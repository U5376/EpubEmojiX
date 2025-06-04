use std::fs::File;
use std::io::{Read, Write};
use zip::{ZipArchive, ZipWriter, write::FileOptions};
use unicode_segmentation::UnicodeSegmentation;
use base64::Engine;
use emojis;

/// Twemoji CDN 基础 URL
const TWEMOJI_BASE: &str = "https://cdn.jsdelivr.net/gh/twitter/twemoji@14.0.2/assets/72x72/";

/// 图片文件夹名
const EMOJI_IMG_DIR: &str = "emoji_img";

/// 判断字符是否为 emoji（简化版）
fn is_emoji_grapheme(g: &str) -> bool {
    emojis::get(g).is_some()
}

/// 将 emoji 字符转为 twemoji 图片 url
fn emoji_to_url(emoji: &str) -> String {
    let codepoints: Vec<String> = emoji.chars().map(|c| format!("{:x}", c as u32)).collect();
    format!("{}{}.png", TWEMOJI_BASE, codepoints.join("-"))
}

/// 替换 xhtml 内容中的 emoji 为 <img> 标签
fn emoji_to_img_tag(emoji: &str) -> String {
    let codepoints: Vec<String> = emoji.chars().map(|c| format!("{:x}", c as u32)).collect();
    let filename = format!("{}.png", codepoints.join("-"));
    format!("<img alt=\"{}\" src=\"{}/{}\" style=\"height:1em;vertical-align:-0.1em\"/>", emoji, EMOJI_IMG_DIR, filename)
}

fn replace_emoji_in_xhtml(xhtml: &str) -> String {
    let mut result = String::new();
    for g in xhtml.graphemes(true) {
        if is_emoji_grapheme(g) {
            let img_tag = emoji_to_img_tag(g);
            result.push_str(&img_tag);
        } else {
            result.push_str(g);
        }
    }
    result
}

/// 替换 epub 文件中的 emoji 为图片
pub fn replace_emoji_in_epub_impl(
    input_path: &str,
    output_path: &str,
) -> Result<(), String> {
    use std::collections::HashSet;
    let input_file = File::open(input_path).map_err(|e| e.to_string())?;
    let mut zip = ZipArchive::new(input_file).map_err(|e| e.to_string())?;
    let mut buffer_map = vec![];
    let mut emoji_imgs = HashSet::new();
    let emoji_dir = "emoji_img";
    // 先遍历所有文件，处理 xhtml/html
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        if name.ends_with(".xhtml") || name.ends_with(".html") {
            if let Ok(orig_str) = String::from_utf8(buf.clone()) {
                // 收集所有 emoji
                for g in orig_str.graphemes(true) {
                    if is_emoji_grapheme(g) {
                        let codepoints: Vec<String> = g.chars().map(|c| format!("{:x}", c as u32)).collect();
                        let filename = format!("{}.png", codepoints.join("-"));
                        emoji_imgs.insert(filename);
                    }
                }
                let replaced = replace_emoji_in_xhtml_auto(&orig_str, emoji_dir);
                buffer_map.push((name, replaced.into_bytes()));
                continue;
            }
        }
        buffer_map.push((name, buf));
    }
    // 写回新 epub
    let out_file = File::create(output_path).map_err(|e| e.to_string())?;
    let mut writer = ZipWriter::new(out_file);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in &buffer_map {
        writer.start_file(name, options).map_err(|e| e.to_string())?;
        writer.write_all(data).map_err(|e| e.to_string())?;
    }
    // 插入 emoji 图片资源
    for filename in emoji_imgs {
        let img_path = format!("{}/{}", emoji_dir, filename);
        if let Ok(mut img_file) = File::open(&img_path) {
            let mut img_data = Vec::new();
            img_file.read_to_end(&mut img_data).map_err(|e| e.to_string())?;
            writer.start_file(format!("{}/{}", emoji_dir, filename), options).map_err(|e| e.to_string())?;
            writer.write_all(&img_data).map_err(|e| e.to_string())?;
        } else {
            // 忽略缺失图片（理论上不会发生）
        }
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn replace_emoji_in_xhtml_auto(xhtml: &str, emoji_dir: &str) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    use std::fs::File;
    use std::io::Read;
    use std::io::Write;
    use std::path::Path;
    let mut result = String::new();
    for g in xhtml.graphemes(true) {
        if is_emoji_grapheme(g) {
            let codepoints: Vec<String> = g.chars().map(|c| format!("{:x}", c as u32)).collect();
            let filename = format!("{}.png", codepoints.join("-"));
            let img_path = format!("{}/{}", emoji_dir, filename);
            let img_tag = if Path::new(&img_path).exists() {
                format!("<img alt=\"{}\" src=\"{}/{}\" style=\"height:1em;vertical-align:-0.1em\"/>", g, emoji_dir, filename)
            } else {
                // 下载图片并保存到本地
                let url = emoji_to_url(g);
                match download_and_save(&url, &img_path) {
                    Ok(_) => format!("<img alt=\"{}\" src=\"{}/{}\" style=\"height:1em;vertical-align:-0.1em\"/>", g, emoji_dir, filename),
                    Err(_) => g.to_string(),
                }
            };
            result.push_str(&img_tag);
        } else {
            result.push_str(g);
        }
    }
    result
}

fn download_and_save(url: &str, path: &str) -> Result<(), String> {
    let resp = reqwest::blocking::get(url).map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("下载失败: {}", url));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    // 创建父目录
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(())
}

pub mod replacer {
    pub use super::replace_emoji_in_epub_impl;
}