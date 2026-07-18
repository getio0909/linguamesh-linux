use std::fmt;

use linguamesh_document::{DocumentError, DocumentJob};

/// 限制通过原生文本导入进入编辑器的最大字节数。
pub const MAX_TEXT_FILE_BYTES: usize = 4 * 1024 * 1024;

/// 表示文本文件导入失败的安全原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextImportError {
    /// 文件超过了编辑器导入上限。
    TooLarge,
    /// 文件不是有效的 UTF-8 文本。
    InvalidUtf8,
    /// 文件后缀不属于当前 Linux 文档切片。
    UnsupportedFormat,
}

impl fmt::Display for TextImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => {
                formatter.write_str("The selected text file exceeds the 4 MiB limit.")
            }
            Self::InvalidUtf8 => formatter.write_str("The selected file is not valid UTF-8 text."),
            Self::UnsupportedFormat => {
                formatter.write_str("The selected document format is not supported.")
            }
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

/// 使用 Core 文档契约检查 TXT/Markdown，并返回保留原始换行的源文本。
pub fn decode_document_contents(
    source_name: &str,
    contents: &[u8],
) -> Result<String, TextImportError> {
    Ok(decode_document_job(source_name, contents)?.source_text())
}

/// 使用 Core 文档契约解码并分段，供持久化文档任务复用同一份快照。
pub fn decode_document_job(
    source_name: &str,
    contents: &[u8],
) -> Result<DocumentJob, TextImportError> {
    let job = DocumentJob::from_utf8(source_name, contents).map_err(map_document_error)?;
    Ok(job)
}

fn map_document_error(error: DocumentError) -> TextImportError {
    match error {
        DocumentError::TooLarge | DocumentError::OutputTooLarge => TextImportError::TooLarge,
        DocumentError::InvalidUtf8 => TextImportError::InvalidUtf8,
        DocumentError::UnsupportedFormat
        | DocumentError::UnknownSegment(_)
        | DocumentError::VerbatimSegment(_)
        | DocumentError::SegmentIncomplete(_) => TextImportError::UnsupportedFormat,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_TEXT_FILE_BYTES, TextImportError, decode_document_contents, decode_document_job,
        decode_text_contents,
    };

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

    #[test]
    fn decodes_through_core_document_contract() {
        assert_eq!(
            decode_document_contents("README.md", b"# Title\r\n\r\ntext"),
            Ok("# Title\r\n\r\ntext".to_owned())
        );
        assert_eq!(
            decode_document_contents("README.docx", b"text"),
            Err(TextImportError::UnsupportedFormat)
        );
    }

    #[test]
    fn returns_a_persistable_document_job() {
        let job = decode_document_job("notes.txt", b"one\ntwo").expect("document job");
        assert_eq!(job.source_name, "notes.txt");
        assert_eq!(job.pending_count(), 2);
    }
}
