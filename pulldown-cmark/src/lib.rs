// Copyright 2015 Google Inc. All rights reserved.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

//! [CommonMark](https://commonmark.org)的拉式解析器（pull parser）。此crate提供了一个[Parser](struct.Parser.html)结构体，
//! 它是[Event](enum.Event.html)的迭代器。该迭代器可以直接使用，
//! 也可以通过[HTML模块](html/index.html)输出HTML。
//!
//! 默认情况下，只启用CommonMark功能。要使用表格、脚注或任务列表等扩展功能，
//! 请在[Options](struct.Options.html)结构体中设置相应的标志来启用它们。
//!
//! # 示例
//! ```rust
//! use pulldown_cmark::{Parser, Options};
//!
//! let markdown_input = "Hello world, this is a ~~complicated~~ *very simple* example.";
//!
//! // 设置选项和解析器。删除线不是CommonMark标准的一部分，
//! // 因此我们必须显式启用它。
//! let mut options = Options::empty();
//! options.insert(Options::ENABLE_STRIKETHROUGH);
//! let parser = Parser::new_ext(markdown_input, options);
//!
//! # #[cfg(feature = "html")] {
//! // 写入字符串缓冲区。
//! let mut html_output = String::new();
//! pulldown_cmark::html::push_html(&mut html_output, parser);
//!
//! // 检查输出是否符合预期。
//! let expected_html = "<p>Hello world, this is a <del>complicated</del> <em>very simple</em> example.</p>\n";
//! assert_eq!(expected_html, &html_output);
//! # }
//! ```
//!
//! 请注意，由于解析器评估源文本的方式，可能会出现连续的文本事件。
//! 提供了`TextMergeStream`工具来改善事件迭代的便利性：
//!
//! ```rust
//! use pulldown_cmark::{Event, Parser, TextMergeStream};
//!
//! let markdown_input = "Hello world, this is a ~~complicated~~ *very simple* example.";
//!
//! let iterator = TextMergeStream::new(Parser::new(markdown_input));
//!
//! for event in iterator {
//!     match event {
//!         Event::Text(text) => println!("{}", text),
//!         _ => {}
//!     }
//! }
//! ```
//!
#![warn(
    clippy::alloc_instead_of_core,
    clippy::std_instead_of_alloc,
    clippy::std_instead_of_core
)]
// 当为rustc编译器本身编译时，我们要确保这是一个不稳定的crate。
#![cfg_attr(rustbuild, feature(staged_api, rustc_private))]
#![cfg_attr(rustbuild, unstable(feature = "rustc_private", issue = "27812"))]
// 除非启用SIMD功能，否则禁止不安全代码。
#![cfg_attr(not(feature = "simd"), forbid(unsafe_code))]
#![warn(missing_debug_implementations)]
#![cfg_attr(not(feature = "std"), no_std)]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

#[cfg(all(not(feature = "std"), not(feature = "hashbrown")))]
compile_error!("\"hashbrown\" feature should be enabled in \"no_std\" environment.");

use alloc::vec::Vec;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "html")]
pub mod html;

pub mod utils;
pub mod chunk;

mod entities;
mod firstpass;
mod linklabel;
mod parse;
mod puncttable;
mod scanners;
mod strings;
mod tree;

use core::fmt::Display;

pub use crate::{
    parse::{
        BrokenLink, BrokenLinkCallback, DefaultParserCallbacks, OffsetIter, Parser,
        ParserCallbacks, RefDefs,
    },
    strings::{CowStr, InlineStr},
    utils::*,
};

/// 代码块类型。
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CodeBlockKind<'a> {
    Indented,
    /// 标签中包含的值描述代码的语言，可能为空。
    #[cfg_attr(feature = "serde", serde(borrow))]
    Fenced(CowStr<'a>),
}

impl<'a> CodeBlockKind<'a> {
    pub fn is_indented(&self) -> bool {
        matches!(*self, CodeBlockKind::Indented)
    }

    pub fn is_fenced(&self) -> bool {
        matches!(*self, CodeBlockKind::Fenced(_))
    }

    pub fn into_static(self) -> CodeBlockKind<'static> {
        match self {
            CodeBlockKind::Indented => CodeBlockKind::Indented,
            CodeBlockKind::Fenced(s) => CodeBlockKind::Fenced(s.into_static()),
        }
    }
}

/// 引用块类型（Note、Tip、Important、Warning、Caution）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BlockQuoteKind {
    Note,        // 注意
    Tip,         // 提示
    Important,   // 重要
    Warning,     // 警告
    Caution,     // 警示
}

/// 容器块类型（仅限Spoiler）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ContainerKind {
    Default,  // 默认
    Spoiler,  // 剧透
}

/// 元数据块类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum MetadataBlockKind {
    YamlStyle,   // YAML风格
    PlusesStyle, // 加号风格
}

/// 可以包含其他元素的标签。
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Tag<'a> {
    /// 文本和其他内联元素的段落。
    Paragraph,

    /// 标题，带有可选的标识符、类和自定义属性。
    /// 标识符以`#`为前缀，选择属性列表中的最后一个，
    /// 类以`.`为前缀，自定义属性没有前缀，可以选择性地有值（`myattr`或`myattr=myvalue`）。
    ///
    /// `id`、`classes`和`attrs`仅在使用[`Options::ENABLE_HEADING_ATTRIBUTES`]时解析和填充，否则为`None`或空。
    Heading {
        level: HeadingLevel,
        id: Option<CowStr<'a>>,
        classes: Vec<CowStr<'a>>,
        /// 元组的第一个项是属性，第二个项是值。
        attrs: Vec<(CowStr<'a>, Option<CowStr<'a>>)>,
    },

    /// 引用块。
    ///
    /// `BlockQuoteKind`仅在使用[`Options::ENABLE_GFM`]时解析和填充，否则为`None`。
    ///
    /// ```markdown
    /// > 常规引用
    ///
    /// > [!NOTE]
    /// > 注意引用
    /// ```
    BlockQuote(Option<BlockQuoteKind>),
    /// 代码块。
    CodeBlock(CodeBlockKind<'a>),
    ContainerBlock(ContainerKind, CowStr<'a>),

    /// HTML块。
    ///
    /// 以某些预定义标签（HTML块标签）开始的行（详见[CommonMark规范](https://spec.commonmark.org/0.31.2/#html-blocks)），
    /// 或任何仅后面跟空白字符的标签。
    ///
    /// 大多数HTML块在空行结束，但一些如`<pre>`、`<script>`或`<!-- Comments -->`等不会。
    /// ```markdown
    /// <body> 这里是非空白字符，但仍然是HTML块。
    /// 块在空行结束。
    ///
    /// <some-random-tag>
    /// 这是HTML块。
    ///
    /// <pre> 在空行处不结束。
    ///
    /// 这仍然是同一个块。</pre>
    /// ```
    HtmlBlock,

    /// 列表。如果列表是有序的，字段指示第一项的编号。
    /// 仅包含列表项。
    List(Option<u64>), // TODO: 为AST添加分隔符和紧凑模式（HTML不需要）
    /// 列表项。
    Item,
    /// 脚注定义。包含的值是脚注的标签，通过该标签可以引用它。
    ///
    /// 仅在使用[`Options::ENABLE_FOOTNOTES`]或[`Options::ENABLE_OLD_FOOTNOTES`]时解析和发出。
    #[cfg_attr(feature = "serde", serde(borrow))]
    FootnoteDefinition(CowStr<'a>),

    /// 仅在使用[`Options::ENABLE_DEFINITION_LIST`]时解析和发出。
    DefinitionList,
    /// 仅在使用[`Options::ENABLE_DEFINITION_LIST`]时解析和发出。
    DefinitionListTitle,
    /// 仅在使用[`Options::ENABLE_DEFINITION_LIST`]时解析和发出。
    DefinitionListDefinition,

    /// 表格。包含描述每列文本对齐方式的向量。
    /// 仅在使用[`Options::ENABLE_TABLES`]时解析和发出。
    Table(Vec<Alignment>),
    /// 表头。仅包含`TableCell`。请注意，表体在`TableHead`标签关闭后立即开始。
    /// 没有`TableBody`标签。
    /// 仅在使用[`Options::ENABLE_TABLES`]时解析和发出。
    TableHead,
    /// 表格行。用于表头行和表体行。仅包含`TableCell`。
    TableRow,
    /// 仅在使用[`Options::ENABLE_TABLES`]时解析和发出。
    TableCell,

    // 跨度级别标签
    /// [Emphasis](https://spec.commonmark.org/0.31.2/#emphasis-and-strong-emphasis).
    /// ```markdown
    /// half*emph* _strong_ _multi _level__
    /// ```
    Emphasis,
    /// [Strong emphasis](https://spec.commonmark.org/0.31.2/#emphasis-and-strong-emphasis).
    /// ```markdown
    /// half**strong** __strong__ __multi __level____
    /// ```
    Strong,
    /// Only parsed and emitted with [`Options::ENABLE_STRIKETHROUGH`].
    ///
    /// ```markdown
    /// ~strike through~
    /// ```
    Strikethrough,
    /// Only parsed and emitted with [`Options::ENABLE_SUPERSCRIPT`].
    ///
    /// ```markdown
    /// ^superscript^
    /// ```
    Superscript,
    /// Only parsed and emitted with [`Options::ENABLE_SUBSCRIPT`], if disabled `~something~` is parsed as [`Strikethrough`](Self::Strikethrough).
    /// ```markdown
    /// ~subscript~ ~~if also enabled this is strikethrough~~
    /// ```
    Subscript,

    /// 链接。
    Link {
        link_type: LinkType,
        dest_url: CowStr<'a>,
        title: CowStr<'a>,
        /// 引用链接的标识符，例如链接`[hello][world]`中的`world`。
        id: CowStr<'a>,
    },

    /// 图片。第一个字段是链接类型，第二个是目标URL，第三个是标题，
    /// 第四个是链接标识符。
    Image {
        link_type: LinkType,
        dest_url: CowStr<'a>,
        title: CowStr<'a>,
        /// 引用链接的标识符，例如链接`[hello][world]`中的`world`。
        id: CowStr<'a>,
    },

    /// 元数据块。
    /// 仅在使用[`Options::ENABLE_YAML_STYLE_METADATA_BLOCKS`]
    /// 或[`Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS`]时解析和发出。
    MetadataBlock(MetadataBlockKind),
}

impl<'a> Tag<'a> {
    pub fn to_end(&self) -> TagEnd {
        match self {
            Tag::Paragraph => TagEnd::Paragraph,
            Tag::Heading { level, .. } => TagEnd::Heading(*level),
            Tag::BlockQuote(kind) => TagEnd::BlockQuote(*kind),
            Tag::CodeBlock(_) => TagEnd::CodeBlock,
            Tag::ContainerBlock(kind, _) => TagEnd::ContainerBlock(*kind),
            Tag::HtmlBlock => TagEnd::HtmlBlock,
            Tag::List(number) => TagEnd::List(number.is_some()),
            Tag::Item => TagEnd::Item,
            Tag::FootnoteDefinition(_) => TagEnd::FootnoteDefinition,
            Tag::Table(_) => TagEnd::Table,
            Tag::TableHead => TagEnd::TableHead,
            Tag::TableRow => TagEnd::TableRow,
            Tag::TableCell => TagEnd::TableCell,
            Tag::Subscript => TagEnd::Subscript,
            Tag::Superscript => TagEnd::Superscript,
            Tag::Emphasis => TagEnd::Emphasis,
            Tag::Strong => TagEnd::Strong,
            Tag::Strikethrough => TagEnd::Strikethrough,
            Tag::Link { .. } => TagEnd::Link,
            Tag::Image { .. } => TagEnd::Image,
            Tag::MetadataBlock(kind) => TagEnd::MetadataBlock(*kind),
            Tag::DefinitionList => TagEnd::DefinitionList,
            Tag::DefinitionListTitle => TagEnd::DefinitionListTitle,
            Tag::DefinitionListDefinition => TagEnd::DefinitionListDefinition,
        }
    }

    pub fn into_static(self) -> Tag<'static> {
        match self {
            Tag::Paragraph => Tag::Paragraph,
            Tag::Heading {
                level,
                id,
                classes,
                attrs,
            } => Tag::Heading {
                level,
                id: id.map(|s| s.into_static()),
                classes: classes.into_iter().map(|s| s.into_static()).collect(),
                attrs: attrs
                    .into_iter()
                    .map(|(k, v)| (k.into_static(), v.map(|s| s.into_static())))
                    .collect(),
            },
            Tag::BlockQuote(k) => Tag::BlockQuote(k),
            Tag::CodeBlock(kb) => Tag::CodeBlock(kb.into_static()),
            Tag::ContainerBlock(k, s) => Tag::ContainerBlock(k, s.into_static()),
            Tag::HtmlBlock => Tag::HtmlBlock,
            Tag::List(v) => Tag::List(v),
            Tag::Item => Tag::Item,
            Tag::FootnoteDefinition(a) => Tag::FootnoteDefinition(a.into_static()),
            Tag::Table(v) => Tag::Table(v),
            Tag::TableHead => Tag::TableHead,
            Tag::TableRow => Tag::TableRow,
            Tag::TableCell => Tag::TableCell,
            Tag::Emphasis => Tag::Emphasis,
            Tag::Strong => Tag::Strong,
            Tag::Strikethrough => Tag::Strikethrough,
            Tag::Superscript => Tag::Superscript,
            Tag::Subscript => Tag::Subscript,
            Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            } => Tag::Link {
                link_type,
                dest_url: dest_url.into_static(),
                title: title.into_static(),
                id: id.into_static(),
            },
            Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            } => Tag::Image {
                link_type,
                dest_url: dest_url.into_static(),
                title: title.into_static(),
                id: id.into_static(),
            },
            Tag::MetadataBlock(v) => Tag::MetadataBlock(v),
            Tag::DefinitionList => Tag::DefinitionList,
            Tag::DefinitionListTitle => Tag::DefinitionListTitle,
            Tag::DefinitionListDefinition => Tag::DefinitionListDefinition,
        }
    }
}

/// `Tag`的结束标记。
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TagEnd {
    Paragraph,
    Heading(HeadingLevel),

    BlockQuote(Option<BlockQuoteKind>),
    CodeBlock,
    ContainerBlock(ContainerKind),

    HtmlBlock,

    /// A list, `true` for ordered lists.
    List(bool),
    Item,
    FootnoteDefinition,

    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,

    Table,
    TableHead,
    TableRow,
    TableCell,

    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,

    Link,
    Image,

    MetadataBlock(MetadataBlockKind),
}

/// 确保`TagEnd`的大小不超过两个字节。
/// 这就是使用它而不是直接使用`Tag`的原因。
#[cfg(target_pointer_width = "64")]
const _STATIC_ASSERT_TAG_END_SIZE: [(); 2] = [(); core::mem::size_of::<TagEnd>()];

impl<'a> From<Tag<'a>> for TagEnd {
    fn from(value: Tag) -> Self {
        value.to_end()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum HeadingLevel {
    H1 = 1,
    H2,
    H3,
    H4,
    H5,
    H6,
}

impl Display for HeadingLevel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::H1 => write!(f, "h1"),
            Self::H2 => write!(f, "h2"),
            Self::H3 => write!(f, "h3"),
            Self::H4 => write!(f, "h4"),
            Self::H5 => write!(f, "h5"),
            Self::H6 => write!(f, "h6"),
        }
    }
}

/// 当尝试将`usize`转换为`Heading`但失败时返回的错误类型，
/// 原因是该usize值不是有效的标题级别
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct InvalidHeadingLevel(usize);

impl TryFrom<usize> for HeadingLevel {
    type Error = InvalidHeadingLevel;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::H1),
            2 => Ok(Self::H2),
            3 => Ok(Self::H3),
            4 => Ok(Self::H4),
            5 => Ok(Self::H5),
            6 => Ok(Self::H6),
            _ => Err(InvalidHeadingLevel(value)),
        }
    }
}

/// 内联链接的类型说明符。更多信息请参见[Tag::Link](enum.Tag.html#variant.Link)。
#[derive(Clone, Debug, PartialEq, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LinkType {
    /// 内联链接，如`[foo](bar)`
    Inline,
    /// 引用链接，如`[foo][bar]`
    Reference,
    /// 文档中没有目标的引用链接，通过broken_link_callback解析
    ReferenceUnknown,
    /// 折叠链接，如`[foo][]`
    Collapsed,
    /// 文档中没有目标的折叠链接，通过broken_link_callback解析
    CollapsedUnknown,
    /// 快捷链接，如`[foo]`
    Shortcut,
    /// 文档中没有目标的快捷链接，通过broken_link_callback解析
    ShortcutUnknown,
    /// 自动链接，如`<http://foo.bar/baz>`
    Autolink,
    /// 自动链接中的电子邮件地址，如`<john@example.org>`
    Email,
    /// 维基链接，如`[[foo]]`或`[[foo|bar]]`
    WikiLink {
        /// 如果维基链接是管道形式的则为`true`。
        ///
        /// * `true` - `[[foo|bar]]`
        /// * `false` - `[[foo]]`
        has_pothole: bool,
    },
}

impl LinkType {
    /// 将链接类型映射到等效的未知链接类型。
    fn to_unknown(self) -> Self {
        match self {
            LinkType::Reference => LinkType::ReferenceUnknown,
            LinkType::Collapsed => LinkType::CollapsedUnknown,
            LinkType::Shortcut => LinkType::ShortcutUnknown,
            _ => unreachable!(),
        }
    }
}

/// 在文档树的前序遍历中生成的Markdown事件，
/// 当内部节点的所有子节点都被访问完毕时，会产生额外的`End`事件。
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Event<'a> {
    /// 标记元素的开始。在此事件之后和其对应的`End`事件之前产生的事件都在此元素内部。
    /// 开始和结束事件保证是平衡的。
    #[cfg_attr(feature = "serde", serde(borrow))]
    Start(Tag<'a>),
    /// 标记元素的结束。
    End(TagEnd),
    /// 文本节点。
    ///
    /// [`Tag`]外部和内部的所有文本。
    #[cfg_attr(feature = "serde", serde(borrow))]
    Text(CowStr<'a>),
    /// An [inline code node](https://spec.commonmark.org/0.31.2/#code-spans).
    ///
    /// ```markdown
    /// `code`
    /// ```
    #[cfg_attr(feature = "serde", serde(borrow))]
    Code(CowStr<'a>),
    /// An inline math environment node.
    /// Requires [`Options::ENABLE_MATH`].
    ///
    /// ```markdown
    /// $math$
    /// ```
    #[cfg_attr(feature = "serde", serde(borrow))]
    InlineMath(CowStr<'a>),
    /// A display math environment node.
    /// Requires [`Options::ENABLE_MATH`].
    ///
    /// ```markdown
    /// $$math$$
    /// ```
    #[cfg_attr(feature = "serde", serde(borrow))]
    DisplayMath(CowStr<'a>),
    /// An HTML node.
    ///
    /// A line of HTML inside [`Tag::HtmlBlock`] includes the line break.
    #[cfg_attr(feature = "serde", serde(borrow))]
    Html(CowStr<'a>),
    /// An [inline HTML node](https://spec.commonmark.org/0.31.2/#raw-html).
    ///
    /// 仅包含标签本身，例如`<open-tag>`、`</close-tag>`或`<!-- comment -->`。
    ///
    /// **注意**：在某些条件下，HTML也可以被解析为HTML块，详情请参见[`Tag::HtmlBlock`]。
    #[cfg_attr(feature = "serde", serde(borrow))]
    InlineHtml(CowStr<'a>),
    /// 对具有给定标签的脚注的引用，由带有[`Tag::FootnoteDefinition`]标签的事件定义。
    /// 定义和引用可以按任意顺序出现。仅在使用[`Options::ENABLE_FOOTNOTES`]或[`Options::ENABLE_OLD_FOOTNOTES`]时解析和发出。
    ///
    /// ```markdown
    /// [^1]
    /// ```
    #[cfg_attr(feature = "serde", serde(borrow))]
    FootnoteReference(CowStr<'a>),
    /// A [soft line break](https://spec.commonmark.org/0.31.2/#soft-line-breaks).
    ///
    /// 任何不是[`HardBreak`](Self::HardBreak)的换行符，或者例如段落的结尾。
    SoftBreak,
    /// A [hard line break](https://spec.commonmark.org/0.31.2/#hard-line-breaks).
    ///
    /// 由至少两个空格或`\`字符前置的行结尾。
    ///
    /// ```markdown
    /// hard··
    /// line\
    /// breaks
    /// ```
    /// *`·`是一个空格*
    HardBreak,
    /// A horizontal ruler.
    ///
    /// ```markdown
    /// ***
    /// ···---
    /// _·_··_····_··
    /// ```
    /// *`·`是任何空白字符*
    Rule,
    /// 任务列表标记，在HTML中呈现为复选框。选中时包含true。
    /// 仅在使用[`Options::ENABLE_TASKLISTS`]时解析和发出。
    /// ```markdown
    /// - [ ] unchecked
    /// - [x] checked
    /// ```
    TaskListMarker(bool),
}

impl<'a> Event<'a> {
    pub fn into_static(self) -> Event<'static> {
        match self {
            Event::Start(t) => Event::Start(t.into_static()),
            Event::End(e) => Event::End(e),
            Event::Text(s) => Event::Text(s.into_static()),
            Event::Code(s) => Event::Code(s.into_static()),
            Event::InlineMath(s) => Event::InlineMath(s.into_static()),
            Event::DisplayMath(s) => Event::DisplayMath(s.into_static()),
            Event::Html(s) => Event::Html(s.into_static()),
            Event::InlineHtml(s) => Event::InlineHtml(s.into_static()),
            Event::FootnoteReference(s) => Event::FootnoteReference(s.into_static()),
            Event::SoftBreak => Event::SoftBreak,
            Event::HardBreak => Event::HardBreak,
            Event::Rule => Event::Rule,
            Event::TaskListMarker(b) => Event::TaskListMarker(b),
        }
    }
}

/// 表格列文本对齐方式。
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]

pub enum Alignment {
    /// 默认文本对齐方式。
    None,
    Left,
    Center,
    Right,
}

bitflags::bitflags! {
    /// 包含用于启用额外功能的标志的选项结构体，
    /// 这些功能不是CommonMark规范的一部分。
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Options: u32 {
        const ENABLE_TABLES = 1 << 1;
        /// GitHub兼容的脚注语法。
        ///
        /// 脚注使用`[^IDENT]`语法引用，
        /// 并在顶级使用标识符后跟冒号定义。
        ///
        /// ---
        ///
        /// ```markdown
        /// Footnote referenced [^1].
        ///
        /// [^1]: footnote defined
        /// ```
        ///
        /// Footnote referenced [^1].
        ///
        /// [^1]: footnote defined
        const ENABLE_FOOTNOTES = 1 << 2;
        const ENABLE_STRIKETHROUGH = 1 << 3;
        const ENABLE_TASKLISTS = 1 << 4;
        /// 启用将ASCII标点字符替换为
        /// Unicode连字和智能引号。
        ///
        /// 这包括将`--`替换为`—`，`---`替换为`—`，`...`替换为`…`，
        /// `"quote"`替换为`"quote"`，以及`'quote'`替换为`'quote'`。
        ///
        /// 替换在文档解析过程中进行。
        const ENABLE_SMART_PUNCTUATION = 1 << 5;
        /// 允许标题具有ID和类的扩展。
        ///
        /// `# text { #id .class1 .class2 myattr other_attr=myvalue }`
        /// 被解释为级别1的标题，
        /// 内容为`text`，ID为`id`，类为`class1`和`class2`，以及
        /// 自定义属性`myattr`（无值）和
        /// 值为`myvalue`的`other_attr`。
        /// 注意，ID、类和自定义属性应该用空格分隔。
        const ENABLE_HEADING_ATTRIBUTES = 1 << 6;
        /// YAML风格的元数据块，即：
        /// - 以`---`行开始
        /// - 以`---`或`...`行结束
        const ENABLE_YAML_STYLE_METADATA_BLOCKS = 1 << 7;
        /// 由以下内容分隔的元数据块：
        /// - 开始处的`+++`行
        /// - 结束处的`+++`行
        const ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS = 1 << 8;
        /// 较旧的脚注语法。此标志意味着`ENABLE_FOOTNOTES`，将其更改为使用
        /// 较旧的语法，而不是新的、默认的GitHub兼容语法。
        ///
        /// 新语法与旧语法在缩进、嵌套和没有定义的脚注引用方面有所不同：
        ///
        /// ```markdown
        /// [^1]: In new syntax, this is two footnote definitions.
        /// [^2]: In old syntax, this is a single footnote definition with two lines.
        ///
        /// [^3]:
        ///
        ///     In new syntax, this is a footnote with two paragraphs.
        ///
        ///     In old syntax, this is a footnote followed by a code block.
        ///
        /// 在新语法中，这个未定义的脚注定义呈现为
        /// 字面文本[^4]。在旧语法中，它创建一个悬空链接。
        /// ```
        const ENABLE_OLD_FOOTNOTES = (1 << 9) | (1 << 2);
        /// 启用此功能后，会发出两个事件`Event::InlineMath`和`Event::DisplayMath`，
        /// 它们按惯例包含TeX公式。
        const ENABLE_MATH = 1 << 10;
        /// CommonMark中不支持的其他GitHub风格Markdown功能。
        /// 目前此标志下的功能包括：
        /// - 引用块标签（[!NOTE]、[!TIP]、[!IMPORTANT]、[!WARNING]、[!CAUTION]）。
        const ENABLE_GFM = 1 << 11;
        /// 与Commonmark-HS-Extensions兼容的定义列表。
        ///
        /// ```markdown
        /// title 1
        ///   : definition 1
        ///
        /// title 2
        ///   : definition 2a
        ///   : definition 2b
        /// ```
        const ENABLE_DEFINITION_LIST = 1 << 12;
        const ENABLE_SUPERSCRIPT = 1 << 13;
        const ENABLE_SUBSCRIPT = 1 << 14;
        /// Obsidian风格的维基链接。
        const ENABLE_WIKILINKS = 1 << 15;
        /// 冒号分隔的容器扩展块。
        const ENABLE_CONTAINER_EXTENSIONS = 1 << 16;
    }
}

impl Options {
    pub(crate) fn has_gfm_footnotes(&self) -> bool {
        self.contains(Options::ENABLE_FOOTNOTES) && !self.contains(Options::ENABLE_OLD_FOOTNOTES)
    }
}
