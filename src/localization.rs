use crate::model::UiLocale;
use std::collections::BTreeMap;
use std::sync::OnceLock;

struct Catalog {
    messages: BTreeMap<String, String>,
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
            let translation = String::from_utf8_lossy(translation)
                .split('\0')
                .next()
                .unwrap_or_default()
                .to_owned();
            if !translation.is_empty() {
                messages.insert(context.to_owned(), translation);
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
                    messages.insert(context, message);
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
            .cloned()
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

#[cfg(test)]
mod tests {
    use super::{Catalog, text};
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
    fn resolves_every_official_linux_catalog() {
        for locale in UiLocale::ALL {
            assert!(!text(locale, "app.title", "").is_empty());
            assert!(!text(locale, "action.translate", "").is_empty());
            assert!(!text(locale, "status.disconnected", "").is_empty());
            assert!(!text(locale, "status.label", "").is_empty());
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
