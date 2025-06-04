use std::env;
use std::fs::File;
use std::io::{Read, Seek, Write};
use zip::{ZipArchive, ZipWriter, write::FileOptions};
use unicode_segmentation::UnicodeSegmentation;
use emojis;
use quick_xml::Reader;
use quick_xml::events::{Event, BytesStart};
use quick_xml::Writer;
use std::io::Cursor;

/// 替换 epub 文件中的 emoji 为图片
pub fn replace_emoji_in_epub_impl(
    input_path: &str,
    output_path: &str,
) -> Result<(), String> {
    use std::collections::HashSet;
    println!("[epub_emoji_x] 打开输入文件: {}", input_path);
    let input_file = File::open(input_path).map_err(|e| format!("打开输入文件失败: {}", e))?;
    let mut zip = ZipArchive::new(input_file).map_err(|e| format!("解析epub为zip失败: {}", e))?;
    println!("[epub_emoji_x] 成功打开epub并解析zip");
    let mut buffer_map = vec![];
    let mut emoji_imgs = HashSet::new();
    let mut opf_path = None;
    let mut opf_content = None;
    // 查找 opf 路径
    let mut opf_dir = String::new();
    if let Some(path) = find_opf_path_from_container(&mut zip) {
        if let Ok(mut opf_file) = zip.by_name(&path) {
            let mut content = String::new();
            opf_file.read_to_string(&mut content).ok();
            opf_content = Some(content);
        }
        opf_dir = std::path::Path::new(&path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "".to_string());
        opf_path = Some(path);
    }
    // emoji_img 目录放在 opf 同级目录
    let emoji_dir = if opf_dir.is_empty() {
        "emoji_img".to_string()
    } else {
        format!("{}/emoji_img", opf_dir)
    };
    // 先遍历所有文件，处理 xhtml/html
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| format!("读取zip第{}个文件失败: {}", i, e))?;
        let name = file.name().to_string();
        println!("[epub_emoji_x] 处理文件: {}", name);
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| format!("读取文件内容失败: {}", e))?;
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
                let xhtml_dir = std::path::Path::new(&name).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                let img_rel = if opf_dir.is_empty() {
                    "../emoji_img".to_string()
                } else {
                    let x = pathdiff::diff_paths(
                        std::path::Path::new(&emoji_dir),
                        std::path::Path::new(&xhtml_dir)
                    ).unwrap_or_else(|| std::path::PathBuf::from("../emoji_img"));
                    x.to_string_lossy().to_string()
                };
                println!("[epub_emoji_x] 替换emoji: 文件={} 相对图片目录={}", name, img_rel);
                let replaced = replace_emoji_in_xhtml_with_imgdir(&orig_str, &img_rel);
                buffer_map.push((name, replaced.into_bytes()));
                continue;
            } else {
                println!("[epub_emoji_x] 文件utf8解码失败: {}", name);
            }
        }
        // 非 xhtml/html 文件直接原样写回
        buffer_map.push((name, buf));
    }
    // 更新 opf 清单
    if let (Some(path), Some(content)) = (opf_path, opf_content) {
        println!("[epub_emoji_x] 更新opf清单: {}", path);
        let new_opf = update_opf_manifest(&content, &emoji_imgs, &emoji_dir);
        buffer_map.retain(|(n, _)| n != &path);
        buffer_map.push((path, new_opf.into_bytes()));
    }
    // 写回新 epub
    println!("[epub_emoji_x] 写回新epub: {}", output_path);
    let out_file = File::create(output_path).map_err(|e| format!("创建输出文件失败: {}", e))?;
    let mut writer = ZipWriter::new(out_file);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in &buffer_map {
        writer.start_file(name, options).map_err(|e| format!("写入zip文件失败: {}", e))?;
        writer.write_all(data).map_err(|e| format!("写入zip内容失败: {}", e))?;
    }
    // 插入 emoji 图片资源
    let exe_dir = env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| std::path::PathBuf::from("."));
    for filename in emoji_imgs {
        let local_img_path = exe_dir.join("emoji_img").join(&filename);
        println!("[epub_emoji_x] 插入emoji图片: {}", local_img_path.display());
        if let Ok(mut img_file) = File::open(&local_img_path) {
            let mut img_data = Vec::new();
            img_file.read_to_end(&mut img_data).map_err(|e| e.to_string())?;
            writer.start_file(format!("{}/{}", emoji_dir, filename), options).map_err(|e| e.to_string())?;
            writer.write_all(&img_data).map_err(|e| e.to_string())?;
        } else {
            println!("[epub_emoji_x] emoji图片文件不存在: {}", local_img_path.display());
        }
    }
    writer.finish().map_err(|e| format!("zip写入完成失败: {}", e))?;
    println!("[epub_emoji_x] 处理完成");
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
fn download_and_save(url: &str, filename: &str) -> Result<(), String> {
    use std::path::Path;
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| Path::new(".").to_path_buf());
    let abs_path = exe_dir.join("emoji_img").join(filename);
    // 优先尝试不带 -fe0f 的图片
    if let Some(stripped) = filename.strip_suffix("-fe0f.png") {
        let fallback = exe_dir.join("emoji_img").join(format!("{}.png", stripped));
        if fallback.exists() {
            std::fs::copy(&fallback, &abs_path).map_err(|e| e.to_string())?;
            println!("[epub_emoji_x] 使用无 -fe0f 变体图片: {} -> {}", fallback.display(), abs_path.display());
            return Ok(());
        }
    }
    println!("[epub_emoji_x] 下载emoji图片: {} -> {}", url, abs_path.display());
    if let Ok(resp) = reqwest::blocking::get(url) {
        if resp.status().is_success() {
            let bytes = resp.bytes().map_err(|e| e.to_string())?;
            if let Some(parent) = abs_path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            let mut file = std::fs::File::create(&abs_path).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    if let Some(stripped) = filename.strip_suffix("-fe0f.png") {
        let fallback_url = emoji_to_url_base(stripped);
        let fallback_path = exe_dir.join("emoji_img").join(format!("{}.png", stripped));
        println!("[epub_emoji_x] 尝试下载无 -fe0f 变体图片: {} -> {}", fallback_url, fallback_path.display());
        if let Ok(resp) = reqwest::blocking::get(&fallback_url) {
            if resp.status().is_success() {
                let bytes = resp.bytes().map_err(|e| e.to_string())?;
                if let Some(parent) = fallback_path.parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                }
                let mut file = std::fs::File::create(&fallback_path).map_err(|e| e.to_string())?;
                file.write_all(&bytes).map_err(|e| e.to_string())?;
                std::fs::copy(&fallback_path, &abs_path).map_err(|e| e.to_string())?;
                println!("[epub_emoji_x] 下载并使用无 -fe0f 变体图片: {} -> {}", fallback_url, abs_path.display());
                return Ok(());
            }
        }
    }
    Err(format!("下载失败: {}", url))
}

fn emoji_to_url(emoji: &str) -> String {
    let codepoints: Vec<String> = emoji.chars().map(|c| format!("{:x}", c as u32)).collect();
    format!("https://gcore.jsdelivr.net/gh/twitter/twemoji@14.0.2/assets/72x72/{}.png", codepoints.join("-"))
}

fn emoji_to_url_base(codepoint: &str) -> String {
    format!("https://gcore.jsdelivr.net/gh/twitter/twemoji@14.0.2/assets/72x72/{}.png", codepoint)
}

fn replace_emoji_in_xhtml_with_imgdir(xhtml: &str, imgdir: &str) -> String {
    let mut result = String::new();
    for g in xhtml.graphemes(true) {
        if is_emoji_grapheme(g) {
            let codepoints: Vec<String> = g.chars().map(|c| format!("{:x}", c as u32)).collect();
            let filename = format!("{}.png", codepoints.join("-"));
            let img_tag = format!("<img alt=\"{}\" src=\"{}/{}\" style=\"height:1em;vertical-align:-0.1em\"/>\n", g, imgdir, filename);
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

fn is_emoji_grapheme(g: &str) -> bool {
    emojis::get(g).is_some()
}

pub mod replacer {
    pub use super::replace_emoji_in_epub_impl;
}