---
name: marp-style
description: >
    Create Marp presentation slide decks in Markdown. Use this skill whenever the user wants to make slides, a presentation, a deck, or a slideshow — even if they don't say "Marp" explicitly. Also trigger for requests like "研究発表のスライドを作って", "プレゼン資料を作りたい", "勉強会のスライドを作って", "スライドのドラフトを作って", or any context where the output is a slide deck. This skill enforces the user's established Marp format conventions (base theme, 4:3, paginate) so always load it for slide creation tasks.
---

# Marp Slide Creation

Always generate a Marp slide file using the conventions below. Do not invent a different frontmatter or theme — use exactly what is specified here.

## Fixed Frontmatter

Every slide file must begin with this exact frontmatter:

```markdown
---
marp: true
theme: base
paginate: true
---
```

## Required Slide Structure

Every deck must follow this order:

### Slide 1 — Title (lead class)

```markdown
<!-- _class: lead -->

# Presentation Title

YYYY 年 M 月 D 日

Affiliation / Presenter
```

### Slide 2 — Table of Contents (toc class)

The `header` directive sets the running header for all subsequent slides.

```markdown
<!--
_class: toc
header: Presentation Title
-->

# 目次

- Section 1
- Section 2
- Section 3
```

### Content Slides

Each content slide starts with `# Heading` (becomes a section title with underline from CSS).

```markdown
---

# Section Title

- Point A
- Point B
    - Sub-point

**Key insight:** use `<b>text</b>` or `**text**` for emphasis.
```

### References Slide (if needed)

```markdown
---

# 参考文献

1. Author, "Title," Venue, Year.
2. ...
```

## Slide Separator

Use `---` on its own line to separate slides.

## CSS Classes and Their Usage

| Class | Purpose | How to apply |
| --- | --- | --- |
| `lead` | Title slide — centered layout, no underline on h1 | `<!-- _class: lead -->` |
| `toc` | Table of contents — vertically centered list | `<!--\n_class: toc\nheader: Title\n-->` |
| `photo-reserve` | Reserves the top half for a photo/image placeholder | `<!-- _class: photo-reserve -->` |
| `fullimage` | Full-bleed image, zero padding | `<!-- _class: fullimage -->` |

Apply a class only to the slide it belongs to using `<!-- _class: ... -->` (underscore prefix = applies only to that one slide).

## Inline HTML

Inline HTML elements are allowed and commonly used in this codebase:

```markdown
<div style="text-align: center; margin: 1.2em 0; font-size: 1.25em;">
<b>Key message goes here</b>
</div>

<div style="margin-top: 0.8em;">
→ Takeaway sentence
</div>
```

## Blockquote = Bottom Citation

A `>` blockquote renders as a small-font citation strip at the bottom of the slide (not a normal quote):

```markdown
> Smith et al., "Paper Title," ICSE 2024.
```

Use this for citing sources on a specific slide.

## Theme Design (base.css)

The `base` theme is defined in `assets/base.css` in this skill directory (also lives at `~/Library/Mobile Documents/iCloud~md~obsidian/Documents/furedea/Slides/base.css`).

Key design tokens:

- Primary color: `rgb(14, 131, 253)` (blue)
- Background: `rgb(239, 255, 255)` (light cyan)
- Code background: `rgb(255, 229, 193)` (peach)
- Body font: Noto Serif JP, 28px
- h1: 50px with bottom border
- h2: 45px, h3: 40px, h4: 35px

## Save Location

Save completed slides to:

```
~/Library/Mobile Documents/iCloud~md~obsidian/Documents/furedea/Slides/
```

Use a descriptive Japanese or English filename with underscores:

- `研究進捗_2026_03.md`
- `tech_talk_rust.md`

## Complete Minimal Example

```markdown
---
marp: true
theme: base
size: 4:3
paginate: true
---

<!-- _class: lead -->

# Rust 入門

2026 年 3 月 18 日

---

<!--
_class: toc
header: Rust 入門
-->

# 目次

- Rust とは
- 所有権システム
- まとめ

---

# Rust とは

- Mozilla が開発したシステムプログラミング言語
- **メモリ安全** をコンパイル時に保証
- GC なし，ゼロコスト抽象化

---

# 所有権システム

- 各値に **唯一の所有者** が存在する
- 所有者がスコープを外れると値は破棄される
- 借用（`&`）で参照を渡せる

---

# まとめ

- Rust = 安全 + 高速
- 所有権モデルがメモリ安全の鍵
- 学習コストは高いが見返りは大きい
```
