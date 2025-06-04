use clap::Parser;
use epub_emoji_x::replace_emoji_in_epub;
use std::ffi::CString;

/// 命令行参数
#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// 输入 epub 文件路径
    #[arg(short, long)]
    input: String,
    /// 输出 epub 文件路径
    #[arg(short, long)]
    output: String,
}

fn main() {
    let args = Args::parse();
    let input = CString::new(args.input).unwrap();
    let output = CString::new(args.output).unwrap();
    let code = replace_emoji_in_epub(input.as_ptr(), output.as_ptr());
    if code == 0 {
        println!("处理完成");
    } else {
        eprintln!("处理失败，错误码: {}", code);
    }
}
