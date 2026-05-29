---
title: Hello, blog-rs
subtitle: A first post on a self-hosted, block-oriented engine
tags: [sample, meta, rust]
status: draft
---

Every blog engine has a "hello world" post. This one doubles as a tour of the
three building blocks you will reach for in almost every article: a callout to
flag something the reader should not miss, a runnable code block to ground the
prose in something concrete, and an image with a caption to break up the
vertical wall of text.

The engine itself is a single Rust binary. Posts are plain markdown files with
a YAML frontmatter header. Anything richer than CommonMark is expressed as a
Hugo-style shortcode, parsed by a small registry, and rendered into HTML
alongside an asset manifest that lists the CSS and JavaScript the page
actually needs. Pages that have no charts ship no Chart.js. Pages with no
code blocks ship no CodeMirror.

{{< callout type="info" >}}
If you are reading the rendered HTML rather than the source, every block on
this page corresponds to one shortcode. View source on the markdown to see
the one-to-one mapping.
{{< /callout >}}

That is the whole authoring story. The rest of this post is a short tour of
why the three blocks below exist in their current form.

## A code block that means something

Code blocks are the heart of a technical blog. The engine's `code` shortcode
wraps a fenced source listing with a `<figure class="code-block">` element,
escapes the body, and tags the figure with the language. If the language is
`rust` and the `playground` flag is set, the figure also gets a
`data-playground="rust"` attribute that the client picks up to wire a "run on
play.rust-lang.org" button. Other languages get the same syntax highlighting
treatment but no playground hook.

Here is a fragment that demonstrates the smallest possible Tokio program. It
spawns a task, waits for it to complete, and prints the result. Nothing
exotic, but it compiles.

{{< code lang="rust" playground="true" >}}
use tokio::task;

#[tokio::main]
async fn main() {
    let handle = task::spawn(async {
        let mut sum: u64 = 0;
        for i in 0..1_000_000u64 {
            sum = sum.wrapping_add(i);
        }
        sum
    });

    let total = handle.await.expect("task panicked");
    println!("sum = {total}");
}
{{< /code >}}

The point of putting the playground hook on `rust` blocks specifically, and
not on every language, is that the Rust playground is a known service with a
stable API. Adding more backends is a future concern; one good integration
beats four wobbly ones.

## Why images get their own block

You could write an `<img>` tag inline. The reason the engine has a dedicated
`image` shortcode is that almost every real-world post needs three things at
once: a caption, an explicit aspect ratio so the layout does not reflow when
the image loads, and a lazy-loading attribute so the browser does not block
the initial paint on every figure at once. The shortcode rolls those three
into one place so individual posts do not have to remember.

{{< image src="/m/architecture-overview.png" alt="A high-level diagram of the blog-rs request pipeline" caption="Markdown enters on the left, HTML and an asset manifest leave on the right." aspect="16/9" >}}

The caption is escaped, the aspect ratio is set as an inline `aspect-ratio`
style so the layout reserves space before the image bytes arrive, and the
image itself is marked `loading="lazy"` so images far below the fold do not
contend with the visible ones.

## Callouts, briefly

The example near the top of the post was an `info` callout. The same
shortcode supports three other types: `tip` for "here is something useful",
`warn` for "this will trip you up", and `danger` for "this will lose you
data". They all share one CSS file, which the renderer only emits when a
post actually uses a callout.

{{< callout type="tip" >}}
The `type` argument defaults to `info`, but writing it out explicitly is
worth the four extra keystrokes. Future-you reading the markdown will thank
present-you.
{{< /callout >}}

## What comes next

Future posts will reach for the heavier blocks: charts to plot benchmark
numbers, `animate` to draw the reader's eye to a sequence of code states,
embed to pull in a YouTube talk, and `playable` to give readers a full
Rust playground iframe without leaving the page. Each one is in the same
shape as the three above: a paired or self-closing shortcode, a handful of
arguments, and an asset entry that only shows up on pages that need it.

Welcome to the blog.
