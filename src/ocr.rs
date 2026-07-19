//! Linux 可选 OCR 插件的受限外部进程边界。

use linguamesh_domain::OperationId;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// OCR 输入 PDF 和输出文本共享 Linux 文档导入的 4 MiB 上限。
pub const MAX_OCR_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// OCR 最多处理前 64 页，避免外部渲染进程无限占用资源。
pub const MAX_OCR_PAGES: usize = 64;
/// 单页渲染图像的大小上限。
pub const MAX_OCR_PAGE_BYTES: u64 = 8 * 1024 * 1024;
/// 单页 OCR 文本的大小上限。
pub const MAX_OCR_PAGE_TEXT_BYTES: usize = 512 * 1024;
/// 全部 OCR 文本的大小上限。
pub const MAX_OCR_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
/// 每个外部进程的最长运行时间。
pub const OCR_PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

/// 一个保留页码关系的 OCR 结果页。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OcrPage {
    /// 从一开始计数的 PDF 页码。
    pub page: usize,
    /// 当前页识别出的 UTF-8 文本。
    pub text: String,
}

/// OCR 插件可能返回的安全、固定类别错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OcrError {
    /// 系统没有安装所需的可选外部工具。
    Unavailable,
    /// 外部工具无法读取或渲染该 PDF。
    InvalidDocument,
    /// 文档超过页数边界。
    TooManyPages,
    /// 外部工具超过时间限制。
    TimedOut,
    /// 外部工具输出超过边界。
    OutputTooLarge,
    /// 文档没有识别出文字。
    NoText,
    /// 外部工具或临时目录操作失败。
    Failed,
}

impl std::fmt::Display for OcrError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Unavailable => "Optional OCR tools are unavailable.",
            Self::InvalidDocument => "The PDF could not be rendered for OCR.",
            Self::TooManyPages => "The PDF has too many pages for OCR.",
            Self::TimedOut => "OCR timed out before completing.",
            Self::OutputTooLarge => "The OCR output exceeds the safety limit.",
            Self::NoText => "OCR did not find readable text.",
            Self::Failed => "The optional OCR process failed.",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for OcrError {}

/// 描述一个可插拔的 Linux OCR 实现。
pub trait OcrPlugin {
    /// 返回用于诊断和报告的稳定插件标识。
    fn id(&self) -> &'static str;
    /// 对受限 PDF 进行页级 OCR，并保留页码关系。
    fn recognize_pdf(&self, pdf: &[u8]) -> Result<Vec<OcrPage>, OcrError>;
}

/// 使用系统 `pdftoppm` 和 `tesseract` 的可选 OCR 插件。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TesseractOcr {
    language: String,
}

impl Default for TesseractOcr {
    fn default() -> Self {
        Self {
            language: "eng".to_owned(),
        }
    }
}

impl TesseractOcr {
    /// 创建一个只接受安全语言标识的 Tesseract 插件。
    pub fn new(language: impl Into<String>) -> Result<Self, OcrError> {
        let language = language.into();
        if language.is_empty()
            || language.len() > 32
            || !language
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(OcrError::Failed);
        }
        Ok(Self { language })
    }

    /// 报告可选工具是否能被当前 Linux 环境调用。
    #[must_use]
    pub fn is_available() -> bool {
        command_available("pdftoppm") && command_available("tesseract")
    }
}

impl OcrPlugin for TesseractOcr {
    fn id(&self) -> &'static str {
        "tesseract"
    }

    fn recognize_pdf(&self, pdf: &[u8]) -> Result<Vec<OcrPage>, OcrError> {
        if pdf.len() > MAX_OCR_INPUT_BYTES || !pdf.starts_with(b"%PDF-") {
            return Err(OcrError::InvalidDocument);
        }
        if !Self::is_available() {
            return Err(OcrError::Unavailable);
        }
        let temp = OcrTempDir::new()?;
        let input = temp.path().join("input.pdf");
        let prefix = temp.path().join("page");
        write_private_file(&input, pdf)?;

        let mut render = Command::new("pdftoppm");
        render
            .arg("-png")
            .arg("-r")
            .arg("150")
            .arg("-f")
            .arg("1")
            .arg("-l")
            .arg((MAX_OCR_PAGES + 1).to_string())
            .arg(&input)
            .arg(&prefix);
        run_process(&mut render, 0)?;

        let mut page_files = fs::read_dir(temp.path())
            .map_err(|_| OcrError::Failed)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
            .collect::<Vec<_>>();
        page_files.sort_by_key(|path| page_number(path).unwrap_or(usize::MAX));
        if page_files.is_empty() {
            return Err(OcrError::InvalidDocument);
        }
        if page_files.len() > MAX_OCR_PAGES {
            return Err(OcrError::TooManyPages);
        }

        let mut pages = Vec::new();
        let mut total_output = 0_usize;
        for path in page_files {
            let size = fs::metadata(&path).map_err(|_| OcrError::Failed)?.len();
            if size > MAX_OCR_PAGE_BYTES {
                return Err(OcrError::OutputTooLarge);
            }
            let mut recognize = Command::new("tesseract");
            recognize
                .arg(&path)
                .arg("stdout")
                .arg("-l")
                .arg(&self.language)
                .arg("--psm")
                .arg("3");
            let output = run_process(&mut recognize, MAX_OCR_PAGE_TEXT_BYTES)?;
            let text = String::from_utf8(output).map_err(|_| OcrError::Failed)?;
            let text = text.trim().to_owned();
            if text.is_empty() {
                continue;
            }
            total_output = total_output.saturating_add(text.len());
            if total_output > MAX_OCR_OUTPUT_BYTES {
                return Err(OcrError::OutputTooLarge);
            }
            pages.push(OcrPage {
                page: page_number(&path).ok_or(OcrError::Failed)?,
                text,
            });
        }
        if pages.is_empty() {
            return Err(OcrError::NoText);
        }
        Ok(pages)
    }
}

/// 运行一个无 shell 的外部进程，并限制时间、标准输出和错误内容。
fn run_process(command: &mut Command, max_output: usize) -> Result<Vec<u8>, OcrError> {
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| OcrError::Unavailable)?;
    let deadline = Instant::now() + OCR_PROCESS_TIMEOUT;
    let status = loop {
        match child.try_wait().map_err(|_| OcrError::Failed)? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(OcrError::TimedOut);
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    let mut output = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout
            .read_to_end(&mut output)
            .map_err(|_| OcrError::Failed)?;
    }
    if output.len() > max_output {
        return Err(OcrError::OutputTooLarge);
    }
    if !status.success() {
        return Err(OcrError::InvalidDocument);
    }
    Ok(output)
}

/// 检查外部工具是否能被当前进程通过 PATH 找到。
fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("-v")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// 从渲染文件名中解析页码，拒绝路径和非数字后缀。
fn page_number(path: &Path) -> Option<usize> {
    path.file_stem()?.to_str()?.rsplit_once('-')?.1.parse().ok()
}

/// 写入仅当前用户可读的 OCR 输入文件。
fn write_private_file(path: &Path, contents: &[u8]) -> Result<(), OcrError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| OcrError::Failed)?;
    std::io::Write::write_all(&mut file, contents).map_err(|_| OcrError::Failed)
}

struct OcrTempDir {
    path: PathBuf,
}

impl OcrTempDir {
    fn new() -> Result<Self, OcrError> {
        let path =
            std::env::temp_dir().join(format!("linguamesh-ocr-{}", OperationId::new().as_str()));
        DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|_| OcrError::Failed)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for OcrTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::{OcrError, OcrPlugin, TesseractOcr};

    #[test]
    fn rejects_unsafe_language_identifiers() {
        assert_eq!(TesseractOcr::new("eng;rm -rf /"), Err(OcrError::Failed));
        assert_eq!(TesseractOcr::new(""), Err(OcrError::Failed));
        assert_eq!(
            TesseractOcr::new("chi_sim"),
            Ok(TesseractOcr::new("chi_sim").unwrap())
        );
    }

    #[test]
    fn rejects_non_pdf_input_before_external_processes() {
        assert_eq!(
            TesseractOcr::default().recognize_pdf(b"not a PDF"),
            Err(OcrError::InvalidDocument)
        );
    }

    #[test]
    #[ignore = "需要系统 pdftoppm、tesseract 和显式 fixture 路径"]
    fn recognizes_the_external_fixture() {
        let path = std::env::var_os("LINGUAMESH_OCR_FIXTURE")
            .expect("LINGUAMESH_OCR_FIXTURE must point to a PDF fixture");
        let pdf = std::fs::read(path).expect("OCR fixture must be readable");
        let pages = TesseractOcr::default()
            .recognize_pdf(&pdf)
            .expect("OCR fixture must produce text");
        assert_eq!(pages.len(), 1);
        assert!(pages[0].text.to_ascii_lowercase().contains("linguamesh"));
    }
}
