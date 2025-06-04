use clap::Parser;
use epub_emoji_x::replace_emoji_in_epub;
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
