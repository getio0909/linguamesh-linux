use crate::model::UiLocale;
use std::collections::BTreeMap;
use std::sync::OnceLock;

struct Catalog {
    messages: BTreeMap<String, String>,
}

fn append_field(field: &mut Option<String>, value: &str) {
    field.get_or_insert_with(String::new).push_str(value);
}

impl Catalog {
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

#[derive(Clone, Copy)]
enum Field {
    Context,
    MessageId,
    Message,
}

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
    static ENGLISH: OnceLock<Catalog> = OnceLock::new();
    static SIMPLIFIED_CHINESE: OnceLock<Catalog> = OnceLock::new();
    match locale {
        UiLocale::English => ENGLISH.get_or_init(|| {
            Catalog::from_po(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/en/LC_MESSAGES/linguamesh.po"
            )))
        }),
        UiLocale::SimplifiedChinese => SIMPLIFIED_CHINESE.get_or_init(|| {
            Catalog::from_po(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/l10n/linux/zh-Hans/LC_MESSAGES/linguamesh.po"
            )))
        }),
    }
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
    fn resolves_pinned_simplified_chinese_catalog() {
        assert_eq!(
            text(UiLocale::SimplifiedChinese, "action.translate", "Translate"),
            "翻译"
        );
    }

    #[test]
    fn unknown_keys_use_the_explicit_fallback() {
        assert_eq!(
            text(UiLocale::English, "missing.key", "Fallback"),
            "Fallback"
        );
    }
}
