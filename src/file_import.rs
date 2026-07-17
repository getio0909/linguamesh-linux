use std::fmt;

/// 限制通过原生文本导入进入编辑器的最大字节数。
pub const MAX_TEXT_FILE_BYTES: usize = 4 * 1024 * 1024;

/// 表示文本文件导入失败的安全原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextImportError {
    /// 文件超过了编辑器导入上限。
    TooLarge,
    /// 文件不是有效的 UTF-8 文本。
    InvalidUtf8,
}

impl fmt::Display for TextImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => {
                formatter.write_str("The selected text file exceeds the 4 MiB limit.")
            }
            Self::InvalidUtf8 => formatter.write_str("The selected file is not valid UTF-8 text."),
        }
    }
}

/// 将受限的文件内容解码为编辑器文本，并移除可选的 UTF-8 BOM。
pub fn decode_text_contents(contents: &[u8]) -> Result<String, TextImportError> {
    if contents.len() > MAX_TEXT_FILE_BYTES {
        return Err(TextImportError::TooLarge);
    }
    let contents = contents.strip_prefix(b"\xef\xbb\xbf").unwrap_or(contents);
    String::from_utf8(contents.to_vec()).map_err(|_| TextImportError::InvalidUtf8)
}

#[cfg(test)]
mod tests {
    use super::{MAX_TEXT_FILE_BYTES, TextImportError, decode_text_contents};

    #[test]
    fn decodes_utf8_and_removes_bom() {
        assert_eq!(decode_text_contents(b"\xef\xbb\xbfHello").unwrap(), "Hello");
    }

    #[test]
    fn rejects_invalid_utf8() {
        assert_eq!(
            decode_text_contents(&[0xff]),
            Err(TextImportError::InvalidUtf8)
        );
    }

    #[test]
    fn rejects_contents_above_the_import_limit() {
        let contents = vec![b'x'; MAX_TEXT_FILE_BYTES + 1];
        assert_eq!(
            decode_text_contents(&contents),
            Err(TextImportError::TooLarge)
        );
    }
}
