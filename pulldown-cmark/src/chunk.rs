//! Markdown 分块器
//!
//! 基于 firstpass 解析器的高性能分块实现。
//! 提供将 Markdown 文档分割为顶层块级元素的功能。

use crate::{Event, HeadingLevel, Options, Tag, TagEnd};
use alloc::vec::Vec;

/// 表示一个 Markdown 块
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Chunk {
    /// 块在文档中的序号
    pub index: usize,
    /// 块的原始文本内容
    pub content: String,
    /// 块的类型
    pub kind: ChunkKind,
}

/// 块类型
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ChunkKind {
    /// 标题
    Heading(HeadingLevel),
    /// 段落
    Paragraph,
    /// 代码块
    CodeBlock,
    /// 引用块
    BlockQuote,
    /// 列表
    List,
    /// 表格
    Table,
    /// 水平分割线
    Rule,
    /// 元数据块
    Metadata,
    /// 脚注定义
    Footnote,
    /// 定义列表
    DefinitionList,
    /// 其他类型
    Other,
}

impl ChunkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChunkKind::Heading(_) => "heading",
            ChunkKind::Paragraph => "paragraph",
            ChunkKind::CodeBlock => "code_block",
            ChunkKind::BlockQuote => "blockquote",
            ChunkKind::List => "list",
            ChunkKind::Table => "table",
            ChunkKind::Rule => "rule",
            ChunkKind::Metadata => "metadata",
            ChunkKind::Footnote => "footnote",
            ChunkKind::DefinitionList => "definition_list",
            ChunkKind::Other => "other",
        }
    }
}

/// 分块配置
#[derive(Clone, Debug)]
pub struct ChunkConfig {
    /// 是否保留原始格式（包括换行符等）
    pub preserve_formatting: bool,
    /// 最大块长度（None 表示无限制）
    pub max_length: Option<usize>,
    /// 是否包含空块
    pub include_empty: bool,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            preserve_formatting: false,
            max_length: None,
            include_empty: false,
        }
    }
}

/// Markdown 分块器
///
/// 使用 firstpass 解析器将 Markdown 文档分割为顶层块级元素。
///
/// # 示例
///
/// ```rust
/// use pulldown_cmark::{Options, chunk::{Chunker, ChunkConfig}};
///
/// let markdown = r#"# 标题
///
/// 这是一个段落。
///
/// - 列表项
/// - 另一个列表项"#;
///
/// let config = ChunkConfig::default();
/// let chunks = Chunker::new(markdown, Options::empty(), config).collect::<Vec<_>>();
///
/// assert_eq!(chunks.len(), 3); // 标题、段落、列表
/// ```
#[derive(Debug)]
pub struct Chunker<'a> {
    text: &'a str,
    config: ChunkConfig,
    chunks: Vec<ChunkInfo>,
    current: usize,
}

#[derive(Debug)]
struct ChunkInfo {
    start: usize,
    end: usize,
    kind: ChunkKind,
}

impl<'a> Chunker<'a> {
    /// 创建新的分块器
    pub fn new(text: &'a str, options: Options, config: ChunkConfig) -> Self {
        let chunks = Self::extract_chunks(text, options);

        Self {
            text,
            config,
            chunks,
            current: 0,
        }
    }

    /// 使用默认配置创建分块器
    pub fn with_defaults(text: &'a str, options: Options) -> Self {
        Self::new(text, options, ChunkConfig::default())
    }

    /// 从文本中提取块级元素
    fn extract_chunks(text: &str, options: Options) -> Vec<ChunkInfo> {
        use crate::Parser;

        let parser = Parser::new_ext(text, options);
        let mut chunks = Vec::new();
        let mut depth = 0;
        let mut stack = Vec::new();

        for (event, range) in parser.into_offset_iter() {
            match event {
                Event::Start(tag) => {
                    if Self::is_block_tag(&tag) && depth == 0 {
                        stack.push((tag, range.start));
                    }
                    depth += 1;
                }
                Event::End(tag_end) => {
                    depth -= 1;
                    if depth == 0 {
                        if let Some((start_tag, start)) = stack.pop() {
                            if Self::tags_match(&start_tag, &tag_end) {
                                chunks.push(ChunkInfo {
                                    start,
                                    end: range.end,
                                    kind: Self::tag_to_kind(&start_tag),
                                });
                            }
                        }
                    }
                }
                Event::Rule => {
                    if depth == 0 {
                        chunks.push(ChunkInfo {
                            start: range.start,
                            end: range.end,
                            kind: ChunkKind::Rule,
                        });
                    }
                }
                _ => {}
            }
        }

        chunks
    }

    /// 判断是否为块级标签
    fn is_block_tag(tag: &Tag) -> bool {
        matches!(
            tag,
            Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::FootnoteDefinition(_)
            | Tag::MetadataBlock(_)
            | Tag::Table(_)
            | Tag::DefinitionList
            | Tag::ContainerBlock(_, _)
        )
    }

    /// 检查开始和结束标签是否匹配
    fn tags_match(start: &Tag, end: &TagEnd) -> bool {
        matches!(
            (start, end),
            (Tag::Paragraph, TagEnd::Paragraph)
            | (Tag::Heading { .. }, TagEnd::Heading(_))
            | (Tag::BlockQuote(_), TagEnd::BlockQuote(_))
            | (Tag::CodeBlock(_), TagEnd::CodeBlock)
            | (Tag::HtmlBlock, TagEnd::HtmlBlock)
            | (Tag::List(_), TagEnd::List(_))
            | (Tag::FootnoteDefinition(_), TagEnd::FootnoteDefinition)
            | (Tag::MetadataBlock(_), TagEnd::MetadataBlock(_))
            | (Tag::Table(_), TagEnd::Table)
            | (Tag::DefinitionList, TagEnd::DefinitionList)
            | (Tag::ContainerBlock(_, _), TagEnd::ContainerBlock(_))
        )
    }

    /// 将标签转换为块类型
    fn tag_to_kind(tag: &Tag) -> ChunkKind {
        match tag {
            Tag::Heading { level, .. } => ChunkKind::Heading(*level),
            Tag::Paragraph => ChunkKind::Paragraph,
            Tag::BlockQuote(_) => ChunkKind::BlockQuote,
            Tag::CodeBlock(_) => ChunkKind::CodeBlock,
            Tag::HtmlBlock => ChunkKind::Other,
            Tag::List(_) => ChunkKind::List,
            Tag::FootnoteDefinition(_) => ChunkKind::Footnote,
            Tag::MetadataBlock(_) => ChunkKind::Metadata,
            Tag::Table(_) => ChunkKind::Table,
            Tag::DefinitionList => ChunkKind::DefinitionList,
            Tag::ContainerBlock(_, _) => ChunkKind::Other,
            _ => ChunkKind::Other,
        }
    }

    /// 从原始文本提取内容
    fn extract_content(&self, chunk: &ChunkInfo) -> String {
        if chunk.start >= chunk.end || chunk.end > self.text.len() {
            return String::new();
        }

        let content = &self.text[chunk.start..chunk.end];
        let processed = if self.config.preserve_formatting {
            content.to_string()
        } else {
            content.trim().to_string()
        };

        if let Some(max_len) = self.config.max_length {
            if processed.len() > max_len {
                match processed.char_indices().nth(max_len) {
                    Some((pos, _)) => format!("{}...", &processed[..pos]),
                    None => processed,
                }
            } else {
                processed
            }
        } else {
            processed
        }
    }
}

impl<'a> Iterator for Chunker<'a> {
    type Item = Chunk;

    fn next(&mut self) -> Option<Self::Item> {
        while self.current < self.chunks.len() {
            let chunk_info = &self.chunks[self.current];
            let content = self.extract_content(chunk_info);

            if self.config.include_empty || !content.is_empty() {
                let chunk = Chunk {
                    index: self.current,
                    content,
                    kind: chunk_info.kind.clone(),
                };
                self.current += 1;
                return Some(chunk);
            }

            self.current += 1;
        }

        None
    }
}

/// 便捷函数：分块 Markdown 文本
pub fn chunk_markdown(text: &str, options: Options) -> Vec<Chunk> {
    Chunker::with_defaults(text, options).collect()
}

/// 便捷函数：使用自定义配置分块 Markdown 文本
pub fn chunk_markdown_with_config(text: &str, options: Options, config: ChunkConfig) -> Vec<Chunk> {
    Chunker::new(text, options, config).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_chunking() {
        let markdown = r#"# 标题

段落内容。

## 二级标题

- 列表项1
- 列表项2"#;

        let chunks = chunk_markdown(markdown, Options::empty());

        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].kind, ChunkKind::Heading(HeadingLevel::H1));
        assert_eq!(chunks[1].kind, ChunkKind::Paragraph);
        assert_eq!(chunks[2].kind, ChunkKind::Heading(HeadingLevel::H2));
        assert_eq!(chunks[3].kind, ChunkKind::List);
    }

    #[test]
    fn test_chunk_indices() {
        let markdown = "# 标题\n\n段落";
        let chunks = chunk_markdown(markdown, Options::empty());

        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.index, i);
        }
    }

    #[test]
    fn test_config_options() {
        let markdown = "# 很长的标题内容很长很长";
        let config = ChunkConfig {
            max_length: Some(10),
            ..Default::default()
        };

        let chunks = chunk_markdown_with_config(markdown, Options::empty(), config);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.chars().count() <= 13); // 10 + "..."
        assert!(chunks[0].content.ends_with("..."));
    }

    #[test]
    fn test_code_block_chunking() {
        let markdown = r#"```rust
let x = 42;
```"#;

        let chunks = chunk_markdown(markdown, Options::empty());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].kind, ChunkKind::CodeBlock);
    }

    #[test]
    fn test_blockquote_chunking() {
        let markdown = r#"> 这是引用块
> 包含多行"#;

        let chunks = chunk_markdown(markdown, Options::empty());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].kind, ChunkKind::BlockQuote);
    }
}