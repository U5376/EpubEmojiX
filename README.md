# EpubEmojiX

EpubEmojiX 是一个将 EPUB 电子书中的 emoji 字符自动替换为图片的工具，支持 Windows 独立运行和嵌入调用。支持自动下载 emoji 图片

## 程序执行逻辑与顺序

1. **打开 EPUB 文件**：解析为 zip 包，遍历所有文件。
2. **查找 OPF 路径**：解析 `META-INF/container.xml`，定位 OPF 文件（如 `OEBPS/content.opf`）。
3. **遍历并处理 xhtml/html 文件**：
   - 仅对 `.xhtml` 和 `.html` 文件进行 emoji 替换(opf定为nav排除 因为发现很多阅读器不支持目录图片显示导致图片后的内容都不显示)
   - 检测每个 emoji 字符(基于emojis库来找emoji)，生成对应图片文件名（如 `1f496.png`）。
   - 检查 exe 所在目录下 `emoji_img/` 是否已有图片，无则自动从 Twemoji CDN 下载（gcore.jsdelivr.net），并保存到本地。
   - 替换 emoji 为 `<img ...>` 标签，图片路径为相对 OPF 的 `../emoji_img/xxx.png`。
   - 每个 `<img>` 标签前后自动加换行（`\n`），避免代码黏连。
   - emoji插入默认样式改成style="height:1.3em" 参考例子：<img alt="✳" src="..\emoji_img/2733.png" style="height:1.3em"/>
4. **更新 OPF 清单**：
   - 在 OPF 的 `<manifest>` 区块自动插入所有 emoji 图片资源（`emoji_img/xxx.png`）。
5. **写回 EPUB**：
   - 将所有原文件和处理后的文件写入新 epub。
   - 将 exe 所在目录下 `emoji_img/xxx.png` 复制到 epub 内部 OPF 同级的 `emoji_img/` 目录。
   - 保证 manifest 路径、图片实际内容、xhtml 路径全部正确。
6. **输出新 EPUB 文件**。

## 使用方法

支持直接把epub拖到exe直接执行

### 命令行用法

```sh
EpubEmojiX.exe -i input.epub -o output.epub
```
- `-i` 输入 epub 文件、目录或 @list.txt 文件列表
- `-o` 输出 epub 文件或目录

#### 示例
- 单文件：
  ```sh
  EpubEmojiX.exe -i book.epub -o book_emoji.epub
  ```
- 批量处理目录：
  ```sh
  EpubEmojiX.exe -i books_dir -o output_dir
  ```
- 文件列表：
  ```sh
  EpubEmojiX.exe -i @list.txt -o output_dir
  ```

### 运行要求
- Windows 系统
- 需联网（首次遇到新 emoji 时自动下载图片）
- emoji 图片会保存在 exe 所在目录的 `emoji_img/` 文件夹

### 典型输出结构
```
EpubEmojiX.exe
emoji_img/
  1f496.png
  1f389.png
  ...
output.epub
```

## 注意事项
- 仅处理 `.xhtml` 和 `.html` 文件(排除nav)，其他文件不做修改。
- emoji 图片优先本地复用，无则自动下载。
- 支持 Twemoji CDN（gcore.jsdelivr.net），如需自定义 CDN 可修改源码。

## License
MIT
