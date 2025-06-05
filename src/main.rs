use clap::Parser;
use epubemojix::replace_emoji_in_epub;
use std::ffi::CString;

/// 命令行参数
#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// 输入文件、目录或文件列表（支持 @list.txt 格式）
    #[arg(short = 'i', long = "input", required = true)]
    input: Vec<String>,
    /// 输出目录（批量模式）或输出文件（单文件模式）
    #[arg(short = 'o', long = "output", required = true)]
    output: String,
    /// 处理 html 或 xhtml 文件（直接替换，不打包为epub）
    #[arg(long = "html", default_value_t = false, action = clap::ArgAction::SetTrue)]
    html: bool,
}

fn expand_input_list(inputs: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for item in inputs {
        if item.starts_with('@') {
            if let Ok(list) = std::fs::read_to_string(&item[1..]) {
                for line in list.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        result.push(trimmed.to_string());
                    }
                }
            }
        } else {
            result.push(item.clone());
        }
    }
    result
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // 拖拽或只输入 -i 文件时自动推导输出路径
    if args.len() == 2 {
        let input = &args[1];
        let output = if input.ends_with(".epub") {
            let stem = std::path::Path::new(input).file_stem().unwrap().to_string_lossy();
            let parent = std::path::Path::new(input).parent().unwrap_or_else(|| std::path::Path::new("."));
            format!("{}\\{}_out.epub", parent.display(), stem)
        } else if input.starts_with('@') {
            let outdir = std::path::Path::new(&input[1..]).parent().unwrap_or_else(|| std::path::Path::new("."));
            format!("{}\\output", outdir.display())
        } else if std::fs::metadata(input).map(|m| m.is_dir()).unwrap_or(false) {
            format!("{}\\output", input)
        } else {
            format!("{}_out", input)
        };
        let input_c = CString::new(input.clone()).unwrap();
        let output_c = CString::new(output.clone()).unwrap();
        let code = replace_emoji_in_epub(input_c.as_ptr(), output_c.as_ptr());
        if code == 0 {
            println!("处理完成: {} -> {}", input, output);
        } else {
            eprintln!("处理失败，错误码: {}", code);
        }
        return;
    }
    // 兼容 -i 但无 -o 时，自动推导输出路径
    if args.len() >= 3 && (args[1] == "-i" || args[1] == "--input") && !args.contains(&"-o".to_string()) && !args.contains(&"--output".to_string()) {
        let input = &args[2];
        let output = if input.ends_with(".epub") {
            let stem = std::path::Path::new(input).file_stem().unwrap().to_string_lossy();
            let parent = std::path::Path::new(input).parent().unwrap_or_else(|| std::path::Path::new("."));
            format!("{}\\{}_out.epub", parent.display(), stem)
        } else if input.starts_with('@') {
            let outdir = std::path::Path::new(&input[1..]).parent().unwrap_or_else(|| std::path::Path::new("."));
            format!("{}\\output", outdir.display())
        } else if std::fs::metadata(input).map(|m| m.is_dir()).unwrap_or(false) {
            format!("{}\\output", input)
        } else {
            format!("{}_out", input)
        };
        let input_c = CString::new(input.clone()).unwrap();
        let output_c = CString::new(output.clone()).unwrap();
        let code = replace_emoji_in_epub(input_c.as_ptr(), output_c.as_ptr());
        if code == 0 {
            println!("处理完成: {} -> {}", input, output);
        } else {
            eprintln!("处理失败，错误码: {}", code);
        }
        return;
    }
    let args = Args::parse();
    let input_list = expand_input_list(&args.input);
    let output = &args.output;
    use std::path::Path;

    // 新增：处理 html/xhtml 文件
    if args.html {
        for (_, input) in input_list.iter().enumerate() {
            let output_path = if input_list.len() == 1 {
                output.clone()
            } else {
                // 多文件时输出到目录
                let fname = Path::new(input).file_name().unwrap().to_string_lossy();
                format!("{}\\{}", output, fname)
            };
            match replace_emoji_in_html_file(input, &output_path) {
                Ok(_) => println!("处理完成: {} -> {}", input, output_path),
                Err(e) => eprintln!("处理失败: {} -> {}，错误: {}", input, output_path, e),
            }
        }
        return;
    }

    if input_list.len() == 1 {
        let input = &input_list[0];
        let meta = std::fs::metadata(input);
        if let Ok(meta) = meta {
            if meta.is_dir() {
                // 目录批量模式
                let mut found = false;
                for entry in std::fs::read_dir(input).unwrap() {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    if path.extension().map(|e| e == "epub").unwrap_or(false) {
                        found = true;
                        let input_path = path.to_string_lossy().to_string();
                        let output_path = format!("{}\\{}", output, path.file_name().unwrap().to_string_lossy());
                        let input_c = CString::new(input_path.clone()).unwrap();
                        let output_c = CString::new(output_path.clone()).unwrap();
                        let code = replace_emoji_in_epub(input_c.as_ptr(), output_c.as_ptr());
                        if code == 0 {
                            println!("处理完成: {} -> {}", input_path, output_path);
                        } else {
                            eprintln!("处理失败: {} -> {}，错误码: {}", input_path, output_path, code);
                        }
                    }
                }
                if !found {
                    println!("未找到 epub 文件: {}", input);
                }
                return;
            }
        }
        // 单文件模式
        let output_path = if output.ends_with(".epub") {
            output.clone()
        } else {
            let fname = Path::new(input).file_name().unwrap().to_string_lossy();
            format!("{}\\{}", output, fname)
        };
        let input_c = CString::new(input.clone()).unwrap();
        let output_c = CString::new(output_path.clone()).unwrap();
        let code = replace_emoji_in_epub(input_c.as_ptr(), output_c.as_ptr());
        if code == 0 {
            println!("处理完成: {} -> {}", input, output_path);
        } else {
            eprintln!("处理失败，错误码: {}", code);
        }
        return;
    } else {
        // 多文件批量模式
        for input in &input_list {
            let fname = Path::new(input).file_name().unwrap().to_string_lossy();
            let output_path = if output.ends_with(".epub") && input_list.len() == 1 {
                output.clone()
            } else {
                format!("{}\\{}", output, fname)
            };
            let input_c = CString::new(input.clone()).unwrap();
            let output_c = CString::new(output_path.clone()).unwrap();
            let code = replace_emoji_in_epub(input_c.as_ptr(), output_c.as_ptr());
            if code == 0 {
                println!("处理完成: {} -> {}", input, output_path);
            } else {
                eprintln!("处理失败: {} -> {}，错误码: {}", input, output_path, code);
            }
        }
    }
}

// 新增：处理 html/xhtml 文件的 emoji 替换
fn replace_emoji_in_html_file(input_path: &str, output_path: &str) -> Result<(), String> {
    use std::fs;
    use std::path::Path;

    let content = fs::read_to_string(input_path).map_err(|e| format!("读取文件失败: {}", e))?;
    // 图片目录与输出文件同级 emoji_img
    let _out_dir = Path::new(output_path).parent().unwrap_or_else(|| Path::new("."));
    let imgdir = "emoji_img";
    let imgdir_rel = imgdir; // 相对路径
    let replaced = epubemojix::replacer::replace_emoji_in_xhtml_with_imgdir(&content, imgdir_rel);

    // 确保 emoji_img 目录存在
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| Path::new(".").to_path_buf());
    let emoji_img_dir = exe_dir.join(imgdir);
    if !emoji_img_dir.exists() {
        std::fs::create_dir_all(&emoji_img_dir).map_err(|e| format!("创建图片目录失败: {}", e))?;
    }

    fs::write(output_path, replaced).map_err(|e| format!("写入输出文件失败: {}", e))?;
    Ok(())
}
