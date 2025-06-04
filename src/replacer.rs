use std::fs::File;
use std::io::{Read, Seek, Write};
use zip::{ZipArchive, ZipWriter, write::FileOptions};
use unicode_segmentation::UnicodeSegmentation;
use emojis;
use quick_xml::Reader;
use quick_xml::events::{Event, BytesStart};
use quick_xml::Writer;
use std::io::Cursor;

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
    let mut opf_path = None;
    let mut opf_content = None;
    let mut opf_dir = String::new();
    // 查找 opf 路径
    if let Some(path) = find_opf_path_from_container(&mut zip) {
        opf_dir = std::path::Path::new(&path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "".to_string());
        opf_path = Some(path.clone());
        if let Ok(mut opf_file) = zip.by_name(&path) {
            let mut content = String::new();
            opf_file.read_to_string(&mut content).ok();
            opf_content = Some(content);
        }
    }
    // emoji_dir 设为 opf 同级 emoji_img
    let emoji_dir = "emoji_img".to_string();
    let opf_dir = opf_path.as_ref().and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_string_lossy().to_string())).unwrap_or_else(|| "".to_string());
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
                // 计算图片在 xhtml 中的相对路径（如 ../emoji_img/xxx.png）
                let xhtml_dir = std::path::Path::new(&name).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                let img_rel = if opf_dir.is_empty() {
                    "../emoji_img".to_string()
                } else {
                    let x = pathdiff::diff_paths(&emoji_dir, std::path::Path::new(&xhtml_dir)).unwrap_or_else(|| std::path::PathBuf::from("../emoji_img"));
                    x.to_string_lossy().to_string()
                };
                let replaced = replace_emoji_in_xhtml_with_imgdir(&orig_str, &img_rel);
                buffer_map.push((name, replaced.into_bytes()));
                continue;
            }
        }
        buffer_map.push((name, buf));
    }
    // 更新 opf 清单
    if let (Some(path), Some(content)) = (opf_path, opf_content) {
        let new_opf = update_opf_manifest(&content, &emoji_imgs, &emoji_dir);
        // 只保留新 opf，移除旧 opf
        buffer_map.retain(|(n, _)| n != &path);
        buffer_map.push((path, new_opf.into_bytes()));
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
        let img_path = format!("{}{}{}",
            if emoji_dir.is_empty() { "" } else { &emoji_dir },
            if emoji_dir.is_empty() { "" } else { "/" },
            filename);
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
    let mut result = String::new();
    for g in xhtml.graphemes(true) {
        if is_emoji_grapheme(g) {
            let codepoints: Vec<String> = g.chars().map(|c| format!("{:x}", c as u32)).collect();
            let filename = format!("{}.png", codepoints.join("-"));
            let img_path = format!("{}/{}", emoji_dir, filename);
            let img_tag = if std::path::Path::new(&img_path).exists() {
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
    let path_obj = std::path::Path::new(path);
    if let Some(parent) = path_obj.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(())
}

fn find_opf_path_from_container<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Option<String> {
    let mut container_xml = String::new();
    if let Ok(mut file) = zip.by_name("META-INF/container.xml") {
        file.read_to_string(&mut container_xml).ok()?;
        let mut reader = Reader::from_str(&container_xml);
        reader.trim_text(true);
        let mut buf: Vec<u8> = Vec::new();
        while let Ok(event) = reader.read_event() {
            match event {
                Event::Empty(ref e) | Event::Start(ref e) => {
                    if e.name().as_ref() == b"rootfile" {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"full-path" {
                                return Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }
    }
    None
}

// 新的 replace_emoji_in_xhtml_with_imgdir
fn replace_emoji_in_xhtml_with_imgdir(xhtml: &str, imgdir: &str) -> String {
    let mut result = String::new();
    for g in xhtml.graphemes(true) {
        if is_emoji_grapheme(g) {
            let codepoints: Vec<String> = g.chars().map(|c| format!("{:x}", c as u32)).collect();
            let filename = format!("{}.png", codepoints.join("-"));
            let img_tag = format!("<img alt=\"{}\" src=\"{}/{}\" style=\"height:1em;vertical-align:-0.1em\"/>", g, imgdir, filename);
            result.push_str(&img_tag);
        } else {
            result.push_str(g);
        }
    }
    result
}

// update_opf_manifest: href 只写 emoji_img/xxx.png，且格式化输出
fn update_opf_manifest(opf_content: &str, emoji_imgs: &std::collections::HashSet<String>, emoji_dir: &str) -> String {
    let mut reader = Reader::from_str(opf_content);
    reader.trim_text(true);
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);
    let mut buf: Vec<u8> = Vec::new();
    let mut in_manifest = false;
    let mut already_inserted = std::collections::HashSet::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"manifest" => {
                in_manifest = true;
                writer.write_event(Event::Start(e.clone())).unwrap();
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"manifest" => {
                // 在 </manifest> 前插入所有 emoji 图片
                for filename in emoji_imgs {
                    let id = format!("emoji_{}", filename.replace(".", "_"));
                    if already_inserted.contains(&id) { continue; }
                    let href = format!("{}/{}", emoji_dir, filename);
                    let href = href.trim_start_matches("./").trim_start_matches(".\\");
                    let href = if href.starts_with("emoji_img/") { href.to_string() } else { format!("emoji_img/{}", filename) };
                    let mut elem = BytesStart::new("item");
                    elem.push_attribute(("id", id.as_str()));
                    elem.push_attribute(("href", href.as_str()));
                    elem.push_attribute(("media-type", "image/png"));
                    writer.write_event(Event::Empty(elem)).unwrap();
                    already_inserted.insert(id);
                }
                in_manifest = false;
                writer.write_event(Event::End(e.clone())).unwrap();
            }
            Ok(Event::Eof) => break,
            Ok(ev) => {
                // 跳过已存在的 emoji_img/xx.png
                if in_manifest {
                    if let Event::Empty(ref e) = ev {
                        let mut is_emoji_img = false;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"href" && String::from_utf8_lossy(&attr.value).starts_with(emoji_dir) {
                                is_emoji_img = true;
                                break;
                            }
                        }
                        if is_emoji_img {
                            continue;
                        }
                    }
                }
                writer.write_event(ev).unwrap();
            }
            Err(_) => break,
        }
        buf.clear();
    }
    String::from_utf8(writer.into_inner().into_inner()).unwrap_or_else(|_| opf_content.to_string())
}

pub mod replacer {
    pub use super::replace_emoji_in_epub_impl;
}