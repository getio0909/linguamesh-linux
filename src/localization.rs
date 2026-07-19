use crate::model::UiLocale;
use std::collections::BTreeMap;
use std::sync::OnceLock;

struct Catalog {
    messages: BTreeMap<String, Vec<String>>,
}

#[cfg(test)]
fn append_field(field: &mut Option<String>, value: &str) {
    field.get_or_insert_with(String::new).push_str(value);
}

impl Catalog {
    fn from_mo(source: &[u8]) -> Self {
        let mut messages = BTreeMap::new();
        let Some(magic) = source.get(..4) else {
            return Self { messages };
        };
        let little_endian = match magic {
            [0xde, 0x12, 0x04, 0x95] => true,
            [0x95, 0x04, 0x12, 0xde] => false,
            _ => return Self { messages },
        };
        let Some(version) = read_mo_u32(source, 4, little_endian) else {
            return Self { messages };
        };
        if version != 0 {
            return Self { messages };
        }
        let Some(count) = read_mo_u32(source, 8, little_endian) else {
            return Self { messages };
        };
        let Some(original_table) = read_mo_u32(source, 12, little_endian) else {
            return Self { messages };
        };
        let Some(translation_table) = read_mo_u32(source, 16, little_endian) else {
            return Self { messages };
        };
        for index in 0..count {
            let Some(original_length) = read_mo_u32(
                source,
                original_table.saturating_add(index.saturating_mul(8)),
                little_endian,
            ) else {
                continue;
            };
            let Some(original_offset) = read_mo_u32(
                source,
                original_table
                    .saturating_add(index.saturating_mul(8))
                    .saturating_add(4),
                little_endian,
            ) else {
                continue;
            };
            let Some(translation_length) = read_mo_u32(
                source,
                translation_table.saturating_add(index.saturating_mul(8)),
                little_endian,
            ) else {
                continue;
            };
            let Some(translation_offset) = read_mo_u32(
                source,
                translation_table
                    .saturating_add(index.saturating_mul(8))
                    .saturating_add(4),
                little_endian,
            ) else {
                continue;
            };
            let Some(original) = mo_slice(source, original_offset, original_length) else {
                continue;
            };
            let Some(translation) = mo_slice(source, translation_offset, translation_length) else {
                continue;
            };
            let original = String::from_utf8_lossy(original);
            let Some((context, _message_id)) = original.split_once('\x04') else {
                continue;
            };
            let translations = String::from_utf8_lossy(translation)
                .split('\0')
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            if translations
                .iter()
                .any(|translation| !translation.is_empty())
            {
                messages.insert(context.to_owned(), translations);
            }
        }
        Self { messages }
    }

    #[cfg(test)]
    fn from_po(source: &str) -> Self {
        let mut messages = BTreeMap::new();
        let mut context: Option<String> = None;
        let mut message_id: Option<String> = None;
        let mut message: Option<String> = None;
        let mut field = None;

        for line in source.lines().chain(std::iter::once("")) {
            if line.trim().is_empty() {
                if let (Some(context), Some(message_id), Some(message)) =
                    (context.take(), message_id.take(), message.take())
                    && !message_id.is_empty()
                    && !message.is_empty()
                {
                    messages.insert(context, vec![message]);
                }
                field = None;
                continue;
            }
            if line.starts_with("#, fuzzy") {
                continue;
            }
            let (next_field, value) = if let Some(value) = line.strip_prefix("msgctxt ") {
                (Some(Field::Context), parse_po_string(value))
            } else if let Some(value) = line.strip_prefix("msgid ") {
                (Some(Field::MessageId), parse_po_string(value))
            } else if let Some(value) = line.strip_prefix("msgstr ") {
                (Some(Field::Message), parse_po_string(value))
            } else if line.starts_with('"') {
                (field, parse_po_string(line))
            } else {
                (None, None)
            };
            if let Some(value) = value {
                match next_field {
                    Some(Field::Context) => append_field(&mut context, &value),
                    Some(Field::MessageId) => append_field(&mut message_id, &value),
                    Some(Field::Message) => append_field(&mut message, &value),
                    None => {}
                }
                field = next_field;
            }
        }

        Self { messages }
    }

    fn get(&self, key: &str, fallback: &str) -> String {
        self.messages
            .get(key)
            .and_then(|translations| translations.first().cloned())
            .unwrap_or_else(|| fallback.to_owned())
    }

    // 按目录提供的复数槽位选择翻译，缺失槽位时回退到可用的首个槽位。
    fn get_plural(&self, key: &str, singular: &str, plural: &str, index: usize) -> String {
        let fallback = if index == 0 { singular } else { plural };
        self.messages
            .get(key)
            .and_then(|translations| {
                translations
                    .get(index)
                    .filter(|translation| !translation.is_empty())
                    .or_else(|| {
                        translations
                            .first()
                            .filter(|translation| !translation.is_empty())
                    })
                    .cloned()
            })
            .unwrap_or_else(|| fallback.to_owned())
    }
}

fn read_mo_u32(source: &[u8], offset: usize, little_endian: bool) -> Option<usize> {
    let bytes = source.get(offset..offset.saturating_add(4))?;
    let value = if little_endian {
        u32::from_le_bytes(bytes.try_into().ok()?)
    } else {
        u32::from_be_bytes(bytes.try_into().ok()?)
    };
    usize::try_from(value).ok()
}

fn mo_slice(source: &[u8], offset: usize, length: usize) -> Option<&[u8]> {
    source.get(offset..offset.checked_add(length)?)
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum Field {
    Context,
    MessageId,
    Message,
}

#[cfg(test)]
fn parse_po_string(value: &str) -> Option<String> {
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut parsed = String::with_capacity(value.len());
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            parsed.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            parsed.push(character);
        }
    }
    if escaped {
        parsed.push('\\');
    }
    Some(parsed)
}

fn catalog(locale: UiLocale) -> &'static Catalog {
    static CATALOGS: OnceLock<[Catalog; 12]> = OnceLock::new();
    let catalogs = CATALOGS.get_or_init(|| {
        [
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/en/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/zh-Hans/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/zh-Hant/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/es/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/fr/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/de/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/ja/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/ko/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/pt-BR/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/ru/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/ar/LC_MESSAGES/linguamesh.mo"
            ))),
            Catalog::from_mo(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/hi/LC_MESSAGES/linguamesh.mo"
            ))),
        ]
    });
    &catalogs[UiLocale::ALL
        .iter()
        .position(|candidate| *candidate == locale)
        .unwrap_or_default()]
}

#[must_use]
pub fn text(locale: UiLocale, key: &str, fallback: &str) -> String {
    catalog(locale).get(key, fallback)
}

// 根据 Linux 目录的 gettext 规则计算稳定的复数槽位。
fn plural_index(locale: UiLocale, count: u64) -> usize {
    match locale {
        UiLocale::Arabic => {
            if count == 0 {
                0
            } else if count == 1 {
                1
            } else if count == 2 {
                2
            } else if (3..=10).contains(&(count % 100)) {
                3
            } else if (11..=99).contains(&(count % 100)) {
                4
            } else {
                5
            }
        }
        UiLocale::French => usize::from(count > 1),
        UiLocale::Russian => {
            if count % 10 == 1 && count % 100 != 11 {
                0
            } else if (2..=4).contains(&(count % 10)) && !(12..=14).contains(&(count % 100)) {
                1
            } else {
                2
            }
        }
        UiLocale::SimplifiedChinese
        | UiLocale::TraditionalChinese
        | UiLocale::Japanese
        | UiLocale::Korean => 0,
        UiLocale::English
        | UiLocale::Spanish
        | UiLocale::German
        | UiLocale::BrazilianPortuguese
        | UiLocale::Hindi => usize::from(count != 1),
    }
}

// 返回带有当前界面语言复数形式的本地化模板。
#[must_use]
pub fn text_plural(
    locale: UiLocale,
    key: &str,
    singular: &str,
    plural: &str,
    count: u64,
) -> String {
    catalog(locale).get_plural(key, singular, plural, plural_index(locale, count))
}

#[cfg(test)]
mod tests {
    use super::{Catalog, text, text_plural};
    use crate::model::UiLocale;

    #[test]
    fn parses_context_and_multiline_values() {
        let catalog = Catalog::from_po(
            "msgctxt \"example\"\nmsgid \"Hello\"\nmsgstr \"Hello \"\n\"world\"\n",
        );
        assert_eq!(catalog.get("example", "fallback"), "Hello world");
    }

    #[test]
    fn parses_generated_mo_context_and_translation() {
        let catalog = Catalog::from_mo(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/l10n/linux/zh-Hans/LC_MESSAGES/linguamesh.mo"
        )));
        assert_eq!(
            catalog.get("error.state.missing_source", "fallback"),
            "翻译前请输入源文本。"
        );
    }

    #[test]
    fn resolves_pinned_simplified_chinese_catalog() {
        assert_eq!(
            text(UiLocale::SimplifiedChinese, "action.translate", "Translate"),
            "翻译"
        );
    }

    #[test]
    fn resolves_gettext_plural_slots_for_runtime_locales() {
        assert_eq!(
            text_plural(
                UiLocale::English,
                "document.file_count",
                "{count} file",
                "{count} files",
                1,
            ),
            "{count} file"
        );
        assert_eq!(
            text_plural(
                UiLocale::English,
                "document.file_count",
                "{count} file",
                "{count} files",
                2,
            ),
            "{count} files"
        );
        assert_eq!(
            text_plural(
                UiLocale::SimplifiedChinese,
                "document.file_count",
                "{count} file",
                "{count} files",
                2,
            ),
            "已选择 {count} 个文件"
        );
        assert_eq!(
            text_plural(
                UiLocale::Russian,
                "document.file_count",
                "{count} file",
                "{count} files",
                5,
            ),
            "{count} файлов"
        );
        assert_eq!(
            text_plural(
                UiLocale::Arabic,
                "document.file_count",
                "{count} file",
                "{count} files",
                2,
            ),
            "تم تحديد ملفين ({count})"
        );
    }

    #[test]
    fn resolves_every_official_linux_catalog() {
        for locale in UiLocale::ALL {
            assert!(!text(locale, "app.title", "").is_empty());
            assert!(!text(locale, "action.translate", "").is_empty());
            assert!(!text(locale, "status.disconnected", "").is_empty());
            assert!(!text(locale, "status.label", "").is_empty());
            assert!(!text(locale, "diagnostics.summary", "").is_empty());
            assert!(!text(locale, "action.open_source", "").is_empty());
            assert!(!text(locale, "dialog.open", "").is_empty());
            assert!(!text(locale, "section.provider_profiles", "").is_empty());
            assert!(!text(locale, "action.connect", "").is_empty());
            assert!(!text(locale, "option.source.auto", "").is_empty());
            assert!(!text(locale, "onboarding.stage.starting", "").is_empty());
            assert!(!text(locale, "onboarding.detail.ready", "").is_empty());
            assert!(!text(locale, "error.provider_name_required", "").is_empty());
            assert!(!text(locale, "error.file_too_large", "").is_empty());
            assert!(!text(locale, "error.worker_disconnected", "").is_empty());
            for key in [
                "error.category.internal",
                "error.category.network",
                "error.state.missing_source",
                "error.state.missing_model",
                "error.state.busy",
                "error.state.event_after_terminal",
                "action.document_jobs",
                "action.pause_document",
                "action.resume_document",
                "action.retry_document",
                "action.open_output",
                "action.enable_fallback",
                "action.select_document_job",
                "dialog.document_jobs",
                "status.document_jobs_empty",
                "status.document_job_row",
                "status.document_job_pending",
                "status.document_job_running",
                "status.document_job_paused",
                "status.document_job_completed",
                "status.document_job_cancelled",
                "status.document_job_failed",
                "settings.enable_ocr",
                "tooltip.enable_ocr",
                "status.ocr_running",
                "error.ocr_unavailable",
                "error.ocr_invalid_document",
                "error.ocr_too_many_pages",
                "error.ocr_timed_out",
                "error.ocr_output_too_large",
                "error.ocr_no_text",
                "error.ocr_failed",
                "status.document_paused",
                "status.document_progress",
                "tooltip.document_jobs",
                "tooltip.open_output",
                "error.output_open",
                "error.fallback_profile_required",
                "label.fallback_profile",
                "option.fallback.none",
                "status.fallback_selected",
                "tooltip.fallback",
                "warning.pdf_image_only_pages",
                "warning.pdf_reconstruction_limited",
                "warning.pdf_uncertain_order",
                "warning.subtitle_line_length",
                "warning.subtitle_reading_speed",
            ] {
                assert!(
                    !text(locale, key, "").is_empty(),
                    "missing Linux key: {key}"
                );
            }
        }
    }

    #[test]
    fn unknown_keys_use_the_explicit_fallback() {
        assert_eq!(
            text(UiLocale::English, "missing.key", "Fallback"),
            "Fallback"
        );
    }
}
