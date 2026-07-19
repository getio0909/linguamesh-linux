use std::fmt;

use crate::ocr::OcrPage;
use linguamesh_document::{
    DocumentError, DocumentFormat, DocumentJob, DocumentSegment, DocumentSegmentKind,
};

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

/// 将页级 OCR 结果转换成保留页码标记的可恢复文本任务。
pub fn document_job_from_ocr(
    source_name: &str,
    pages: &[OcrPage],
) -> Result<DocumentJob, TextImportError> {
    if pages.is_empty() {
        return Err(TextImportError::InvalidStructure);
    }
    let source_name = format!("{source_name}.ocr.txt");
    let mut segments = Vec::new();
    let mut index = 0_usize;
    let mut total_bytes = 0_usize;
    for page in pages {
        if page.page == 0 || page.text.is_empty() {
            return Err(TextImportError::InvalidStructure);
        }
        let marker = format!("[OCR page {}]", page.page);
        total_bytes = total_bytes.saturating_add(marker.len() + 1);
        segments.push(DocumentSegment {
            index,
            kind: DocumentSegmentKind::Verbatim,
            source_text: marker,
            translated_text: None,
            line_ending: "\n".to_owned(),
        });
        index = index.saturating_add(1);
        for line in page.text.lines() {
            if line.is_empty() {
                continue;
            }
            total_bytes = total_bytes.saturating_add(line.len() + 1);
            segments.push(DocumentSegment {
                index,
                kind: DocumentSegmentKind::Prose,
                source_text: line.to_owned(),
                translated_text: None,
                line_ending: "\n".to_owned(),
            });
            index = index.saturating_add(1);
        }
    }
    if segments.is_empty() || total_bytes > MAX_TEXT_FILE_BYTES {
        return Err(TextImportError::TooLarge);
    }
    Ok(DocumentJob {
        format: DocumentFormat::Txt,
        source_name,
        segments,
        package: None,
    })
}

fn map_document_error(error: DocumentError) -> TextImportError {
    match error {
        DocumentError::TooLarge | DocumentError::OutputTooLarge => TextImportError::TooLarge,
        DocumentError::InvalidUtf8 => TextImportError::InvalidUtf8,
        DocumentError::InvalidStructure => TextImportError::InvalidStructure,
        DocumentError::UnsupportedFormat
        | DocumentError::UnknownSegment(_)
        | DocumentError::VerbatimSegment(_)
        | DocumentError::SegmentIncomplete(_)
        | DocumentError::PdfTextEncodingUnsupported => TextImportError::UnsupportedFormat,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_TEXT_FILE_BYTES, TextImportError, decode_document_contents, decode_document_job,
        decode_text_contents, document_job_from_ocr,
    };
    use crate::ocr::OcrPage;
    use linguamesh_document::DocumentSegmentKind;
    use std::io::{Cursor, Read, Write};
    use zip::ZipArchive;
    use zip::write::{SimpleFileOptions, ZipWriter};

    fn docx_fixture() -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        writer
            .start_file("[Content_Types].xml", options)
            .expect("content types");
        writer
            .write_all(
                b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>",
            )
            .expect("content types bytes");
        writer
            .start_file("word/document.xml", options)
            .expect("document");
        writer
            .write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Hello &amp; world</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
            )
            .expect("document bytes");
        writer
            .start_file("word/header1.xml", options)
            .expect("header");
        writer
            .write_all(br#"<w:hdr xmlns:w="urn:w"><w:p><w:r><w:t>Header</w:t></w:r></w:p></w:hdr>"#)
            .expect("header bytes");
        writer
            .start_file("word/media/image.bin", options)
            .expect("image");
        writer.write_all(&[1, 2, 3, 4]).expect("image bytes");
        writer.finish().expect("docx archive").into_inner()
    }

    fn xlsx_fixture() -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        writer
            .start_file("[Content_Types].xml", options)
            .expect("content types");
        writer.write_all(b"<Types/>").expect("content types bytes");
        writer
            .start_file("xl/workbook.xml", options)
            .expect("workbook");
        writer
            .write_all(
                br#"<workbook xmlns="urn:x"><sheets><sheet name="Sheet1"/></sheets></workbook>"#,
            )
            .expect("workbook bytes");
        writer
            .start_file("xl/sharedStrings.xml", options)
            .expect("shared strings");
        writer
            .write_all(
                br#"<sst xmlns="urn:x"><si><t>Hello &amp; world</t></si><si><t>Shared</t></si></sst>"#,
            )
            .expect("shared strings bytes");
        writer
            .start_file("xl/worksheets/sheet1.xml", options)
            .expect("worksheet");
        writer
            .write_all(
                br#"<worksheet xmlns="urn:x"><sheetData><row><c t="inlineStr"><is><t>Inline</t></is></c><c t="s"><v>0</v></c><c><f>SUM(A1:A2)</f><v>3</v></c><c><v>42</v></c></row></sheetData></worksheet>"#,
            )
            .expect("worksheet bytes");
        writer
            .start_file("xl/media/image.bin", options)
            .expect("image");
        writer.write_all(&[8, 9, 10]).expect("image bytes");
        writer.finish().expect("xlsx archive").into_inner()
    }

    fn pptx_fixture() -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        writer
            .start_file("[Content_Types].xml", options)
            .expect("content types");
        writer.write_all(b"<Types/>").expect("content types bytes");
        writer
            .start_file("ppt/presentation.xml", options)
            .expect("presentation");
        writer
            .write_all(br#"<p:presentation xmlns:p="urn:ppt"><p:sldMasterIdLst/></p:presentation>"#)
            .expect("presentation bytes");
        writer
            .start_file("ppt/slides/slide1.xml", options)
            .expect("slide");
        writer
            .write_all(
                br#"<p:sld xmlns:p="urn:ppt" xmlns:a="urn:dml"><p:cSld><p:spTree><a:p><a:r><a:t>Slide &amp; title</a:t></a:r></a:p><a:p><a:r><a:t>Body</a:t></a:r></a:p></p:spTree></p:cSld></p:sld>"#,
            )
            .expect("slide bytes");
        writer
            .start_file("ppt/notesSlides/notesSlide1.xml", options)
            .expect("notes");
        writer
            .write_all(
                br#"<p:notes xmlns:p="urn:ppt" xmlns:a="urn:dml"><a:p><a:r><a:t>Speaker note</a:t></a:r></a:p></p:notes>"#,
            )
            .expect("notes bytes");
        writer
            .start_file("ppt/media/image.bin", options)
            .expect("image");
        writer.write_all(&[5, 6, 7]).expect("image bytes");
        writer.finish().expect("pptx archive").into_inner()
    }

    fn traversal_docx_fixture() -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        writer
            .start_file("[Content_Types].xml", options)
            .expect("content types");
        writer.write_all(b"<Types/>").expect("content types bytes");
        writer
            .start_file("../outside.txt", options)
            .expect("traversal entry");
        writer.write_all(b"unsafe").expect("traversal bytes");
        writer
            .start_file("word/document.xml", options)
            .expect("document");
        writer
            .write_all(br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Safe</w:t></w:r></w:p></w:body></w:document>"#)
            .expect("document bytes");
        writer.finish().expect("docx archive").into_inner()
    }

    fn oversized_docx_fixture() -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file("[Content_Types].xml", options)
            .expect("content types");
        writer.write_all(b"<Types/>").expect("content types bytes");
        writer
            .start_file("word/document.xml", options)
            .expect("document");
        writer
            .write_all(br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Safe</w:t></w:r></w:p></w:body></w:document>"#)
            .expect("document bytes");
        writer
            .start_file("word/large.bin", options)
            .expect("large resource");
        writer
            .write_all(&vec![b'x'; MAX_TEXT_FILE_BYTES + 1])
            .expect("large resource bytes");
        writer.finish().expect("docx archive").into_inner()
    }

    fn suspicious_compression_ratio_docx_fixture() -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file("[Content_Types].xml", options)
            .expect("content types");
        writer.write_all(b"<Types/>").expect("content types bytes");
        writer
            .start_file("word/document.xml", options)
            .expect("document");
        writer
            .write_all(br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Safe</w:t></w:r></w:p></w:body></w:document>"#)
            .expect("document bytes");
        writer
            .start_file("word/repetitive.bin", options)
            .expect("repetitive resource");
        writer
            .write_all(&vec![b'x'; 512 * 1024])
            .expect("repetitive resource bytes");
        writer.finish().expect("docx archive").into_inner()
    }

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
        assert_eq!(
            decode_document_contents("slides.pptx", b"not a ZIP package"),
            Err(TextImportError::InvalidStructure)
        );
        assert_eq!(
            decode_document_contents("workbook.xlsx", b"not a ZIP package"),
            Err(TextImportError::InvalidStructure)
        );
        assert_eq!(
            decode_document_contents("book.epub", b"not a ZIP package"),
            Err(TextImportError::InvalidStructure)
        );
        assert_eq!(
            decode_document_contents("book.pdf", b"not a PDF"),
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

    #[test]
    fn creates_page_marked_text_jobs_from_ocr() {
        let job = document_job_from_ocr(
            "scan.pdf",
            &[
                OcrPage {
                    page: 1,
                    text: "First page".to_owned(),
                },
                OcrPage {
                    page: 2,
                    text: "Second page".to_owned(),
                },
            ],
        )
        .expect("OCR document job");
        assert_eq!(job.source_name, "scan.pdf.ocr.txt");
        assert_eq!(job.format, linguamesh_document::DocumentFormat::Txt);
        assert_eq!(job.segments[0].kind, DocumentSegmentKind::Verbatim);
        assert_eq!(job.segments[0].source_text, "[OCR page 1]");
        assert_eq!(job.pending_count(), 2);
        assert_eq!(job.segments[2].source_text, "[OCR page 2]");
    }

    #[test]
    fn imports_docx_and_rebuilds_text_without_dropping_package_parts() {
        let source = docx_fixture();
        let mut job = decode_document_job("sample.docx", &source).expect("docx job");
        assert_eq!(job.pending_count(), 3);
        let prose_indices = job
            .segments
            .iter()
            .enumerate()
            .filter(|(_, segment)| segment.kind == DocumentSegmentKind::Prose)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        for (index, translated) in prose_indices
            .into_iter()
            .zip(["你好 & 世界", "单元格", "页眉"])
        {
            job.apply_translation(index, translated)
                .expect("translation");
        }
        let rebuilt = job.reconstruct_bytes().expect("rebuild docx");
        let mut archive = ZipArchive::new(Cursor::new(rebuilt)).expect("rebuilt archive");
        let mut document = String::new();
        archive
            .by_name("word/document.xml")
            .expect("document entry")
            .read_to_string(&mut document)
            .expect("document xml");
        assert!(document.contains("你好 &amp; 世界"));
        assert!(document.contains("单元格"));
        let mut header = String::new();
        archive
            .by_name("word/header1.xml")
            .expect("header entry")
            .read_to_string(&mut header)
            .expect("header xml");
        assert!(header.contains("页眉"));
        let mut image = Vec::new();
        archive
            .by_name("word/media/image.bin")
            .expect("image entry")
            .read_to_end(&mut image)
            .expect("image bytes");
        assert_eq!(image, [1, 2, 3, 4]);
    }

    #[test]
    fn imports_xlsx_and_preserves_unselected_values_and_formulas() {
        let source = xlsx_fixture();
        let mut job = decode_document_job("sample.xlsx", &source).expect("xlsx job");
        assert_eq!(job.pending_count(), 3);
        let prose_segments = job
            .segments
            .iter()
            .enumerate()
            .filter(|(_, segment)| segment.kind == DocumentSegmentKind::Prose)
            .map(|(index, segment)| (index, segment.source_text.clone()))
            .collect::<Vec<_>>();
        for (index, source_text) in prose_segments {
            let translated = if source_text == "Hello & world" {
                "你好 & 世界"
            } else {
                source_text.as_str()
            };
            job.apply_translation(index, translated)
                .expect("translation");
        }
        let rebuilt = job.reconstruct_bytes().expect("rebuild xlsx");
        let mut archive = ZipArchive::new(Cursor::new(rebuilt)).expect("rebuilt archive");
        let mut shared_strings = String::new();
        archive
            .by_name("xl/sharedStrings.xml")
            .expect("shared strings entry")
            .read_to_string(&mut shared_strings)
            .expect("shared strings xml");
        assert!(shared_strings.contains("你好 &amp; 世界"));
        assert!(shared_strings.contains("Shared"));
        let mut worksheet = String::new();
        archive
            .by_name("xl/worksheets/sheet1.xml")
            .expect("worksheet entry")
            .read_to_string(&mut worksheet)
            .expect("worksheet xml");
        assert!(worksheet.contains("Inline"));
        assert!(worksheet.contains("<f>SUM(A1:A2)</f>"));
        assert!(worksheet.contains("<v>42</v>"));
        let mut image = Vec::new();
        archive
            .by_name("xl/media/image.bin")
            .expect("image entry")
            .read_to_end(&mut image)
            .expect("image bytes");
        assert_eq!(image, [8, 9, 10]);
    }

    #[test]
    fn imports_pptx_and_preserves_notes_and_resources() {
        let source = pptx_fixture();
        let mut job = decode_document_job("sample.pptx", &source).expect("pptx job");
        assert_eq!(job.pending_count(), 3);
        let prose_indices = job
            .segments
            .iter()
            .enumerate()
            .filter(|(_, segment)| segment.kind == DocumentSegmentKind::Prose)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        for index in prose_indices {
            job.apply_translation(index, "译文").expect("translation");
        }
        let rebuilt = job.reconstruct_bytes().expect("rebuild pptx");
        let mut archive = ZipArchive::new(Cursor::new(rebuilt)).expect("rebuilt archive");
        let mut slide = String::new();
        archive
            .by_name("ppt/slides/slide1.xml")
            .expect("slide entry")
            .read_to_string(&mut slide)
            .expect("slide xml");
        assert!(slide.contains("译文"));
        let mut notes = String::new();
        archive
            .by_name("ppt/notesSlides/notesSlide1.xml")
            .expect("notes entry")
            .read_to_string(&mut notes)
            .expect("notes xml");
        assert!(notes.contains("译文"));
        let mut image = Vec::new();
        archive
            .by_name("ppt/media/image.bin")
            .expect("image entry")
            .read_to_end(&mut image)
            .expect("image bytes");
        assert_eq!(image, [5, 6, 7]);
    }

    #[test]
    fn rejects_docx_archive_path_traversal_before_import() {
        assert_eq!(
            decode_document_job("unsafe.docx", &traversal_docx_fixture()),
            Err(TextImportError::InvalidStructure)
        );
    }

    #[test]
    fn rejects_docx_archive_with_oversized_uncompressed_entry() {
        assert_eq!(
            decode_document_job("oversized.docx", &oversized_docx_fixture()),
            Err(TextImportError::TooLarge)
        );
    }

    #[test]
    fn rejects_docx_archive_with_suspicious_compression_ratio() {
        assert_eq!(
            decode_document_job(
                "suspicious-ratio.docx",
                &suspicious_compression_ratio_docx_fixture(),
            ),
            Err(TextImportError::TooLarge)
        );
    }
}
