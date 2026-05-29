---
title: Building blog-rs in the open
subtitle: A meta-post on the engine, with live numbers and live code
tags: [sample, meta, rust, architecture]
status: draft
---

I started writing blog-rs because every off-the-shelf blog engine I tried
forced a choice I did not want to make: either ship a heavy SPA framework on
every page and pay for it in load time, or fall back to plain markdown and
lose the interactive code blocks, charts, and animations that I actually
wanted to write about. This post is a meta-tour of the engine, with the same
engine doing the rendering. The chart below is a live chart. The code block
further down is a live playground. The animated callouts use the same
asset-aware pipeline as every other post.

The goal of the architecture, stated once, is this. A post is a sequence of
typed blocks. The renderer walks the sequence, asks each block for its HTML
and its asset dependencies, and emits a deduplicated manifest. The HTTP
layer reads that manifest to decide which `<link>` and `<script>` tags to
inject. A post that never uses a chart never pays for Chart.js.

## The numbers that motivated the design

The first thing I measured, when prototyping the idea, was the asset cost of
the popular alternative. A typical Ghost or WordPress post lands on the
client weighing several hundred kilobytes of JavaScript and CSS before the
post body has a chance to render. A typical Hugo post is lean but inert.
blog-rs aims at the middle: lean by default, heavy only on pages that opt
in.

{{< chart type="bar" data="[420, 380, 18, 95]" caption="Average page weight in KB for a typical text post. Lower is better." >}}

The four bars are, in order, a default WordPress install, a default Ghost
install, a static Hugo page with no client JS, and a blog-rs page that uses
a callout and a code block. The blog-rs number is higher than Hugo because
it ships the CodeMirror highlighting bundle that the code block requested.
A post with only prose would land closer to Hugo.

Reading time follows page weight closely on slow connections, which is most
of the world. The line chart below pulls live latency-to-interactive numbers
from a synthetic test rig that pings the public demo from five regions every
five minutes.

{{< chart type="line" src="data/latency-by-region.json" caption="P95 time-to-interactive, milliseconds, last 24 hours, by region." >}}

Both charts use the same `chart` shortcode. The first one passes its data
inline as a small JSON literal. The second one points at a file in the
asset tree that the build pipeline writes from a cron-fed Postgres query.
Inline is the right choice for tiny datasets that belong with the prose;
a `src` reference is the right choice for anything live or large.

## A code block you can actually run

The single most useful interactive block, for a programming blog, is a code
sample the reader can edit and run without leaving the page. The engine
supports two flavours of this. The lighter one is a regular code block with
the `playground="true"` flag, which adds a "open in playground" button to
the figure. The heavier one is the dedicated `playable` shortcode, which
embeds the full Rust playground as a sandboxed iframe.

Here is the lighter version. It compiles a small benchmark harness that
measures how long a loop of additions takes, using `std::time::Instant`.
The point of putting it in a runnable block is that the reader can change
the iteration count and immediately see the result.

{{< code lang="rust" playground="true" >}}
use std::time::Instant;

fn main() {
    let iters: u64 = 10_000_000;
    let start = Instant::now();

    let mut acc: u64 = 0;
    for i in 0..iters {
        acc = acc.wrapping_add(i.wrapping_mul(31));
    }

    let elapsed = start.elapsed();
    println!("acc = {acc}");
    println!("{iters} iters in {:?}", elapsed);
}
{{< /code >}}

And here is the heavier version, the full embedded playground, pointed at a
gist that holds a slightly longer example: a tiny async TCP echo server
written against Tokio. The iframe is sandboxed and lazy-loaded, so the cost
is only paid when the reader scrolls it into view.

{{< playable id="rust-playground" gist="a1b2c3d4e5f6" >}}

## A diagram that introduces itself

The last interactive block worth showing is `animate`. It wraps a region of
the post in a `<div data-preset="...">` that a small Motion One driver
animates into view as the reader scrolls past. The preset names are
deliberate: `fade`, `slide-up`, `slide-left`, `scale`, and an escape hatch
called `custom` that takes a `keyframes` argument. For a meta-post like
this one, a single `slide-up` on the architecture summary is enough to draw
the eye.

{{< animate preset="slide-up" >}}
The pipeline, end to end, is six stages. Read the markdown file. Parse the
YAML frontmatter. Lex the body into a stream of markdown segments and
shortcode invocations. Render each shortcode through its registered handler.
Pass the rest of the body through the CommonMark renderer. Concatenate the
HTML in order and dedupe the asset list.
{{< /animate >}}

That paragraph is exactly the same as if it had been written as plain prose,
except that the engine emits a wrapping `<div>` with a preset attribute and
adds two small scripts to the asset manifest. If the reader's browser has
JavaScript disabled, the paragraph still renders as static text. The block
degrades to plain content without any extra work from the author.

## What I left out

This post deliberately skipped two blocks. The `embed` shortcode, for
YouTube and Twitter and arbitrary URLs, gets its own demonstration in the
full tour. The `image` shortcode, which is the workhorse of every post that
needs a screenshot, is covered in the introductory post. The point of this
post was to show the three blocks that make a blog feel alive: a chart that
moves, a code block that runs, and an animation that arrives on cue.

If you want to read the full set in one place, jump to the comprehensive
tour. If you want to know how the renderer is implemented, the source is
in the `crates/content` and `crates/shortcodes` workspaces. There is not
much to it. That is the design goal.
