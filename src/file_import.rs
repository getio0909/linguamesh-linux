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
    /// 文档的结构或字段无效。
    InvalidStructure,
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
            Self::InvalidStructure => {
                formatter.write_str("The selected document structure is invalid.")
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

/// 使用 Core 文档契约检查受支持格式，并返回保留原始换行的源文本。
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
        DocumentError::InvalidStructure => TextImportError::InvalidStructure,
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
            Err(TextImportError::InvalidStructure)
        );
        assert_eq!(
            decode_document_contents("captions.srt", b"1\nnot a timestamp\nHello"),
            Err(TextImportError::InvalidStructure)
        );
        assert_eq!(
            decode_document_contents("table.csv", b"id,name\n1,\"bad\n"),
            Err(TextImportError::InvalidStructure)
        );
        assert_eq!(
            decode_document_contents("payload.json", br#"{"name":"Alice","count":2}"#),
            Ok(r#"{"name":"Alice","count":2}"#.to_owned())
        );
        assert_eq!(
            decode_document_contents("payload.json", br#"{"name":"bad\q"}"#),
            Err(TextImportError::InvalidStructure)
        );
        assert_eq!(
            decode_document_contents("page.html", b"<p>Hello</p>"),
            Ok("<p>Hello</p>".to_owned())
        );
        assert_eq!(
            decode_document_contents("page.html", b"<p>Hello"),
            Err(TextImportError::InvalidStructure)
        );
        assert_eq!(
            decode_document_contents("README.docx", b"not a ZIP package"),
            Err(TextImportError::InvalidStructure)
        );
    }

    #[test]
    fn returns_a_persistable_document_job() {
        let job = decode_document_job("notes.txt", b"one\ntwo").expect("document job");
        assert_eq!(job.source_name, "notes.txt");
        assert_eq!(job.pending_count(), 2);
    }

    #[test]
    fn returns_subtitle_jobs_without_translating_timing_lines() {
        let job = decode_document_job(
            "captions.vtt",
            b"WEBVTT\n\n00:00.000 --> 00:01.000\nHello\n",
        )
        .expect("webvtt job");
        assert_eq!(job.pending_count(), 1);
        assert_eq!(job.segments[2].source_text, "00:00.000 --> 00:01.000");
    }

    #[test]
    fn returns_csv_jobs_with_decoded_translation_source() {
        let job = decode_document_job("comments.csv", b"id,comment\n1,\"Hello, world\"\n")
            .expect("csv job");
        let index = job
            .segments
            .iter()
            .position(|segment| segment.source_text.starts_with("\"Hello"))
            .expect("quoted field");
        assert_eq!(job.translation_source_text(index).unwrap(), "Hello, world");
    }

    #[test]
    fn returns_json_jobs_with_decoded_translation_source() {
        let job = decode_document_job("payload.json", br#"{"name":"Alice","count":2}"#)
            .expect("json job");
        let index = job
            .segments
            .iter()
            .position(|segment| segment.source_text == "\"Alice\"")
            .expect("json value");
        assert_eq!(job.translation_source_text(index).unwrap(), "Alice");
    }

    #[test]
    fn returns_html_jobs_with_visible_text_segments() {
        let job = decode_document_job("page.html", b"<p>Hello <strong>world</strong></p>")
            .expect("html job");
        let prose = job
            .segments
            .iter()
            .filter(|segment| segment.kind == linguamesh_document::DocumentSegmentKind::Prose)
            .map(|segment| segment.source_text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(prose, vec!["Hello ", "world"]);
    }
}
