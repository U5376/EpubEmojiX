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
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 替换 epub 文件中的 emoji 为图片
pub fn replace_emoji_in_epub_impl(
    input_path: &str,
    output_path: &str,
) -> Result<(), String> {
    let mut global_counts: HashMap<String, usize> = HashMap::new();
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
    let mut nav_files = std::collections::HashSet::new();
    if let Some(path) = find_opf_path_from_container(&mut zip) {
        if let Ok(mut opf_file) = zip.by_name(&path) {
            let mut content = String::new();
            opf_file.read_to_string(&mut content).ok();
            // 修正：解析 nav 文件名，拼接并规范化为 zip 内路径
            {
                let opf_dir_path = std::path::Path::new(&path).parent().unwrap_or_else(|| std::path::Path::new(""));
                let mut reader = Reader::from_str(&content);
                reader.trim_text(true);
                let mut buf: Vec<u8> = Vec::new();
                loop {
                    match reader.read_event() {
                        Ok(Event::Empty(ref e)) => {
                            let mut is_nav = false;
                            let mut href_val = None;
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"properties" && attr.value.as_ref() == b"nav" {
                                    is_nav = true;
                                }
                                if attr.key.as_ref() == b"href" {
                                    href_val = Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                            if is_nav {
                                if let Some(href) = href_val {
                                    // 拼接成 zip 内部的完整路径并规范化
                                    let nav_path = opf_dir_path.join(href).components().collect::<PathBuf>();
                                    let nav_path_str = nav_path.to_string_lossy().replace("\\", "/");
                                    nav_files.insert(nav_path_str);
                                }
                            }
                        }
                        Ok(Event::Eof) => break,
                        _ => {}
                    }
                    buf.clear();
                }
            }
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

    // 遍历所有文件，处理 xhtml/html
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| format!("读取zip第{}个文件失败: {}", i, e))?;
        let name = file.name().to_string();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|e| format!("读取文件内容失败: {}", e))?;
        // 修正：排除 nav 文件（路径规范化后比较）
        let name_normalized = name.replace("\\", "/");
        if name.ends_with(".xhtml") || name.ends_with(".html") {
            if nav_files.contains(&name_normalized) {
                println!("[epub_emoji_x] 跳过nav文件: {}", name);
                buffer_map.push((name.clone(), buf.clone()));
                continue;
            }
            if let Ok(orig_str) = String::from_utf8(buf.clone()) {
                // 先局部统计本文件的 emoji 数量
                let mut counts: HashMap<String, usize> = HashMap::new();
                for g in orig_str.graphemes(true) {
                    if is_emoji_grapheme(g) {
                        let code = g.chars()
                            .map(|c| format!("{:x}", c as u32)) // 改为小写
                            .collect::<Vec<_>>()
                            .join("-");
                        let code = code.to_lowercase(); // 统一小写
                        *counts.entry(code.clone()).or_insert(0) += 1;
                        *global_counts.entry(code).or_insert(0) += 1;
                    }
                }
                // 仅当需要替换时，才计算路径 & 执行替换
                if !counts.is_empty() {
                    // 先算出这次文件的 img_rel
                    let xhtml_dir = Path::new(&name)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let img_rel = if opf_dir.is_empty() {
                        "../emoji_img".to_string()
                    } else {
                        pathdiff::diff_paths(
                            Path::new(&emoji_dir),
                            Path::new(&xhtml_dir),
                        )
                        .unwrap_or_else(|| PathBuf::from("../emoji_img"))
                        .to_string_lossy()
                        .to_string()
                    };
        
                    // 打印日志 & 记录要插入的图片
                    let total_file: usize = counts.values().sum();
                    let detail_file = counts.iter()
                        .map(|(code, &n)| format!("{}×{}", code, n))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!(
                        "[epub_emoji_x] 文件={}，共替换 {} 个emoji： {}",
                        name, total_file, detail_file
                    );
                    for code in counts.keys() {
                        emoji_imgs.insert(format!("{}.png", code.to_lowercase())); // 统一小写
                    }
                    // 真正替换并写入 buffer_map
                    let replaced = replace_emoji_in_xhtml_with_imgdir(&orig_str, &img_rel);
                    buffer_map.push((name.clone(), replaced.into_bytes()));
                } else {
                    // 如果没有 emoji，直接原样写回
                    buffer_map.push((name.clone(), buf.clone()));
                }
            } else {
                // UTF-8 解码失败，也原样写回
                println!("[epub_emoji_x] 文件utf8解码失败: {}", name);
                buffer_map.push((name.clone(), buf.clone()));
            }
            continue;
        }
        // 非 .xhtml/.html 文件，原样写回
        buffer_map.push((name.clone(), buf.clone()));
    }
    
    // 在更新 OPF 清单前，打印全局汇总
    if !global_counts.is_empty() {
        let total_all: usize = global_counts.values().sum();
        let distinct_count = global_counts.len();
        let detail_all = global_counts
            .iter()
            .map(|(code, &n)| format!("{}×{}", code, n))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "[epub_emoji_x] 共替换 {} 个emoji，种类数 {}，详细：{}",
            total_all, distinct_count, detail_all
        );
    }
    // 更新 opf 清单
    if let (Some(path), Some(content)) = (opf_path, opf_content) {
        println!("[epub_emoji_x] 更新opf清单: {}", path);
        let new_opf = update_opf_manifest(&content, &emoji_imgs, &emoji_dir);
        buffer_map.retain(|(n, _)| n != &path);
        buffer_map.push((path, new_opf.into_bytes()));
    }
    // 写回新 epub
    println!("[epub_emoji_x] 开始写回epub: {}", output_path);
    let out_file = File::create(output_path).map_err(|e| format!("创建输出文件失败: {}", e))?;
    let mut writer = ZipWriter::new(out_file);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in &buffer_map {
        writer.start_file(name, options).map_err(|e| format!("写入zip文件失败: {}", e))?;
        writer.write_all(data).map_err(|e| format!("写入zip内容失败: {}", e))?;
    }
    // 插入 emoji 图片资源
    let exe_dir = env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| std::path::PathBuf::from("."));
    for filename in emoji_imgs {
        let filename = filename.to_lowercase(); // 统一小写
        let local_img_path = exe_dir.join("emoji_img").join(&filename);
        println!("[epub_emoji_x] 插入emoji图片文件: {}", local_img_path.display());
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
    let filename = filename.to_lowercase(); // 统一小写
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| Path::new(".").to_path_buf());
    let abs_path = exe_dir.join("emoji_img").join(&filename);
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

pub fn replace_emoji_in_xhtml_with_imgdir(xhtml: &str, imgdir: &str) -> String {
    let mut result = String::new();
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| std::path::PathBuf::from("."));
    let imgdir = imgdir.replace("\\", "/");
    for g in xhtml.graphemes(true) {
        if is_emoji_grapheme(g) {
            let codepoints: Vec<String> = g.chars().map(|c| format!("{:x}", c as u32)).collect();
            let filename = format!("{}.png", codepoints.join("-").to_lowercase()); // 统一小写
            let abs_path = exe_dir.join("emoji_img").join(&filename);
            if !abs_path.exists() {
                let url = emoji_to_url(g);
                let _ = download_and_save(&url, &filename);
            }
            let img_tag = format!("\n<img alt=\"{}\" src=\"{}/{}\" style=\"height:1.3em\"/>\n", g, imgdir, filename);
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
                    let filename = filename.to_lowercase(); // 统一小写
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
    pub use super::replace_emoji_in_xhtml_with_imgdir;
}