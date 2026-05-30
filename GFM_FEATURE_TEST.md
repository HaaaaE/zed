# GFM Feature Test

This file is a manual fixture for checking the Markdown editor's GFM support plus the math extension.

## Inline Text

Plain text with **bold**, _emphasis_, ***bold emphasis***, ~~strikethrough~~, `inline code`, an escaped marker \*not emphasis\*, and entities: &amp; &lt; &gt; &quot; &#169; &#x2713;.

Reference links should work too: [full reference][docs], [collapsed reference][], and [shortcut reference].

Autolinks:

<https://github.github.com/gfm/>

<user@example.com>

[docs]: https://github.github.com/gfm/ "GFM Spec"
[collapsed reference]: https://example.com/collapsed
[shortcut reference]: https://example.com/shortcut

## Soft And Hard Breaks

This line wraps with a soft break
and should remain part of the same paragraph.

This line has a hard break.  
This text should start after the hard break.

## Images

Inline image:

![Small placeholder image](https://via.placeholder.com/96x48.png?text=GFM)

Reference image:

![Reference placeholder][placeholder-image]

[placeholder-image]: https://via.placeholder.com/120x60.png?text=Ref

## Blockquotes

> A blockquote paragraph with **inline formatting** and a [link](https://example.com).
>
> - quoted list item
> - [ ] quoted unchecked task
> - [x] quoted checked task
>   1. nested ordered item
>   2. second nested ordered item
>
> > Nested blockquote with `code`.

## Lists

- unordered item
- item with nested content
  - nested unordered item
  - [ ] nested unchecked task
  - [X] nested uppercase checked task
- item after nested content

1. ordered dot item
2. second ordered dot item

1) ordered paren item
2) second ordered paren item

- [ ] unchecked task item
- [x] checked task item
- [X] uppercase checked task item
- [ ]invalid task marker should stay plain text after marker

## Table

| Feature | Status | Notes |
| :--- | :---: | ---: |
| Tables | Supported | left / center / right alignment |
| Tasks | Supported | [x] source marker renders as a checkbox |
| CJK | 支持 | 中文内容用于宽字符检查 |

## Code Blocks

Indented code block:

    fn indented_code() {
        println!("four leading spaces");
    }

Fenced code block:

```rust
fn fenced_code() {
    println!("GFM fenced code");
}
```

Fenced code block with tildes:

~~~text
tilde fence content
~~~

## Raw HTML And Tagfilter

Safe raw HTML should remain visible as text in the editor:

<div data-kind="safe">safe raw HTML text</div>

GFM tagfilter-disallowed raw HTML should also remain visible as raw text:

<script>alert("do not execute");</script>

<iframe src="https://example.com"></iframe>

<style>body { color: red; }</style>

This should not be treated as a disallowed tag:

<scripted-value>safe custom-looking tag name</scripted-value>

## Math Extension

Inline math: $E = mc^2$ and $\alpha + \beta = \gamma$.

Block math with single-dollar style:

$
\int_0^1 x^2\,dx = \frac{1}{3}
$

Block math with double-dollar style:

$$
\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}
$$

## Mixed Stress Paragraph

> - [ ] A quoted task with **bold**, `code`, &amp; entity, CJK 中文, inline math $a^2 + b^2 = c^2$, and an image ![tiny](https://via.placeholder.com/16.png).

End of fixture.
