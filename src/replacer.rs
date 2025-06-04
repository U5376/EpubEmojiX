use unicode_segmentation::UnicodeSegmentation;
use epub::doc::EpubDoc;
use base64::Engine;
use emojis;

/// Twemoji CDN 基础 URL
const TWEMOJI_BASE: &str = "https://cdn.jsdelivr.net/gh/twitter/twemoji@14.0.2/assets/72x72/";

/// 判断字符是否为 emoji（简化版）
fn is_emoji_grapheme(g: &str) -> bool {
    emojis::get(g).is_some()
}

/// 将 emoji 字符转为 twemoji 图片 url
fn emoji_to_url(emoji: &str) -> String {
    let codepoints: Vec<String> = emoji.chars().map(|c| format!("{:x}", c as u32)).collect();
    format!("{}{}.png", TWEMOJI_BASE, codepoints.join("-"))
}

/// 下载图片并返回 base64 字符串
fn download_and_base64(url: &str) -> Result<String, String> {
    let resp = reqwest::blocking::get(url).map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("下载失败: {}", url));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// 替换 xhtml 内容中的 emoji 为 <img> 标签
fn replace_emoji_in_xhtml(xhtml: &str) -> String {
    let mut result = String::new();
    for g in xhtml.graphemes(true) {
        if is_emoji_grapheme(g) {
            let url = emoji_to_url(g);
            let img_tag = match download_and_base64(&url) {
                Ok(b64) => format!("<img alt=\"{}\" src=\"data:image/png;base64,{}\" style=\"height:1em;vertical-align:-0.1em\"/>", g, b64),
                Err(_) => g.to_string(),
            };
            result.push_str(&img_tag);
        } else {
            result.push_str(g);
        }
    }
    result
}

/// 替换 epub 文件中的 emoji 为图片
pub fn replace_emoji_in_epub_impl(input_path: &str, output_path: &str) -> Result<(), String> {
    let mut doc = EpubDoc::new(input_path).map_err(|e| format!("打开epub失败: {e:?}"))?;
    for item in &doc.spine {
        if let Some(id) = item.id.as_deref() {
            if let Some((orig, mime)) = doc.get_resource(id) {
                if mime.contains("html") {
                    let orig_str = String::from_utf8_lossy(&orig);
                    let replaced = replace_emoji_in_xhtml(&orig_str);
                    doc.set_data(id, replaced.as_bytes().to_vec());
                }
            }
        }
    }
    doc.save(output_path).map_err(|e| format!("保存epub失败: {e:?}"))?;
    Ok(())
}

pub mod replacer {
    pub use super::replace_emoji_in_epub_impl;
}