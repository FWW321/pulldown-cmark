//! 演示 pulldown-cmark 分块器的使用方法
//!
//! 这个示例展示了如何使用新的分块功能将 Markdown 文档分割为顺序的块。

use pulldown_cmark::{Options, chunk::{Chunker, ChunkConfig, chunk_markdown, chunk_markdown_with_config}};

fn main() {
    let sample_markdown = r#"# 标题一

这是第一个段落的文本内容。

## 标题二

这是第二个段落。

- 列表项一
- 列表项二
  - 嵌套的列表项
  - 另一个嵌套项
- 列表项三

> 这是一个引用块
> 包含多行内容

```rust
let code = "example";
```

| 列1 | 列2 | 列3 |
|-----|-----|-----|
| A   | B   | C   |
| D   | E   | F   |

最后一段内容。"#;

    println!("原始 Markdown：");
    println!("{}", sample_markdown);
    println!("\n{}\n", "=".repeat(60));

    // 示例1：使用便捷函数进行基本分块
    println!("1. 基本分块（使用便捷函数）：");
    basic_chunking_demo(sample_markdown);

    println!("\n{}\n", "=".repeat(60));

    // 示例2：使用迭代器模式进行分块
    println!("2. 迭代器模式分块：");
    iterator_chunking_demo(sample_markdown);

    println!("\n{}\n", "=".repeat(60));

    // 示例3：使用自定义分块选项
    println!("3. 自定义选项分块：");
    custom_options_chunking_demo(sample_markdown);
}

fn basic_chunking_demo(markdown: &str) {
    let options = Options::ENABLE_TABLES;
    let chunks = chunk_markdown(markdown, options);

    println!("分块结果（共 {} 个块）：", chunks.len());

    for chunk in &chunks {
        println!("块 {}: {:?} - {}", chunk.index, chunk.kind, chunk.content);
    }
}

fn iterator_chunking_demo(markdown: &str) {
    let options = Options::ENABLE_TABLES;
    let config = ChunkConfig::default();
    let chunker = Chunker::new(markdown, options, config);

    println!("使用迭代器模式逐块处理：");

    for chunk in chunker {
        println!("块 {}: {:?} - {}", chunk.index, chunk.kind, chunk.content);

        // 这里可以逐块处理，比如实时分析或流式处理
        match chunk.kind {
            pulldown_cmark::chunk::ChunkKind::Heading(level) => {
                println!("  → 这是 {} 级标题", level as u8);
            }
            pulldown_cmark::chunk::ChunkKind::CodeBlock => {
                println!("  → 这是一个代码块");
            }
            pulldown_cmark::chunk::ChunkKind::List => {
                println!("  → 这是一个列表");
            }
            pulldown_cmark::chunk::ChunkKind::BlockQuote => {
                println!("  → 这是一个引用块");
            }
            _ => {
                println!("  → 其他类型块");
            }
        }
    }
}

fn custom_options_chunking_demo(markdown: &str) {
    let parse_options = Options::ENABLE_TABLES;
    let chunk_config = ChunkConfig {
        include_empty: false,
        preserve_formatting: true,  // 保留格式
        max_length: Some(50), // 限制最大长度
    };

    let chunks = chunk_markdown_with_config(markdown, parse_options, chunk_config);

    println!("自定义选项分块结果（保留格式，限制长度）：");

    for chunk in &chunks {
        println!("块 {}: {:?} - {}", chunk.index, chunk.kind, chunk.content);
        println!("  长度: {} 字符", chunk.content.len());
    }

    // 演示块类型统计
    println!("\n块类型统计：");
    let mut stats = std::collections::HashMap::new();

    for chunk in &chunks {
        let chunk_type_str = chunk.kind.as_str();
        *stats.entry(chunk_type_str).or_insert(0) += 1;
    }

    for (chunk_type, count) in &stats {
        println!("  {}: {} 个", chunk_type, count);
    }
}