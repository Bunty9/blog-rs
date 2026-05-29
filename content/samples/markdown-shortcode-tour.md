---
title: The complete shortcode tour
subtitle: Every built-in block, in one post, used the way you would actually use it
tags: [sample, reference, rust, shortcodes]
status: draft
---

This post is the long-form reference for the shortcode set. It uses every
built-in block at least once, in roughly the order you would reach for them
when drafting a real article. The prose around each block is not filler.
It explains why the block exists in its current shape and what to watch out
for when you use it.

## Callouts in all four colours

The `callout` shortcode is the simplest paired block in the registry. It
takes one argument, `type`, which selects one of four built-in styles. The
default is `info`. The body is parsed as markdown, so links and inline code
work the way you expect.

{{< callout type="info" >}}
The `info` variant is the neutral one. Use it for context, definitions, or
a brief recap of something the reader may have skipped. It is the right
default when you cannot decide.
{{< /callout >}}

{{< callout type="tip" >}}
The `tip` variant is for "here is the easier way". Use it when there is a
shortcut the reader will be glad you mentioned. Inline code like
`cargo install just` renders inside callouts the same as in body text.
{{< /callout >}}

{{< callout type="warn" >}}
The `warn` variant is for "this is going to trip you up". Use it for foot
guns, surprising defaults, or version-specific behaviour. The body is still
markdown, so you can [link out](https://doc.rust-lang.org/cargo/) to the
authoritative source.
{{< /callout >}}

{{< callout type="danger" >}}
The `danger` variant is for "this will destroy data, drop a table, or open
a port to the public internet". Reserve it. If every callout in a post is
red, none of them are.
{{< /callout >}}

All four variants share one CSS file. The renderer emits that file exactly
once into the asset manifest no matter how many callouts the post contains.

## Code blocks, with and without a playground

The `code` shortcode wraps a fenced source listing. The body is escaped, the
language is set on the inner `<code class="language-...">` element for
syntax highlighting, and the figure picks up a `data-playground="rust"`
attribute when both `lang="rust"` and `playground="true"` are present.

A non-Rust block, for contrast. This one is plain shell, with no playground
hook, because there is no playground service for shell.

{{< code lang="bash" >}}
cargo run -p blog-rs-render -- content/samples/markdown-shortcode-tour.md \
    --assets-out /tmp/assets.json \
    --frontmatter-out /tmp/fm.yaml \
    > /tmp/out.html
{{< /code >}}

A Rust block with the playground flag set. The button to open the snippet
on play.rust-lang.org is wired up client-side from the `data-playground`
attribute.

{{< code lang="rust" playground="true" >}}
fn fizzbuzz(n: u32) -> String {
    match (n % 3, n % 5) {
        (0, 0) => "FizzBuzz".to_string(),
        (0, _) => "Fizz".to_string(),
        (_, 0) => "Buzz".to_string(),
        (_, _) => n.to_string(),
    }
}

fn main() {
    for n in 1..=15 {
        println!("{}", fizzbuzz(n));
    }
}
{{< /code >}}

## Images with captions and explicit aspect ratios

The `image` shortcode takes `src`, optional `alt`, optional `caption`, and
optional `aspect`. The aspect ratio is set as an inline CSS variable so the
layout reserves the right amount of vertical space before the image bytes
arrive. That single change cuts cumulative layout shift to roughly zero on
image-heavy pages, which is the largest single Lighthouse win available
without rewriting the markup.

{{< image src="/m/render-pipeline-diagram.png" alt="Diagram showing markdown flowing through frontmatter parsing, lexing, shortcode rendering, and HTML emission" caption="The six-stage render pipeline. Each stage is a pure function." aspect="16/9" >}}

For a portrait image, swap the aspect ratio. The same shortcode handles it.

{{< image src="/m/profile-flamegraph.png" alt="A flamegraph showing CPU time across the render call stack" caption="Flamegraph of a single render call. Markdown parsing dominates; shortcode dispatch is negligible." aspect="3/4" >}}

## Charts, both inline and from a file

The `chart` shortcode supports five chart types (`line`, `bar`, `scatter`,
`radar`, `doughnut`) and two data sources. For tiny datasets that belong
with the prose, pass `data="..."` as inline JSON. For anything that updates
or is more than a handful of points, point at a file with `src="..."`.

A small inline bar chart, showing the rough relative weight of each shortcode
asset bundle on a page that uses everything.

{{< chart type="bar" data="[2, 14, 1, 65, 9]" caption="Approximate KB shipped per block type when present on a page: callout, code (CodeMirror), image, chart (Chart.js + glue), animate (Motion One + glue)." >}}

A larger chart that lives in a file. The file path is symbolic; in
production the build pipeline writes the JSON from a query.

{{< chart type="line" src="data/render-throughput.json" caption="Posts rendered per second by markdown size, measured with criterion." >}}

A doughnut variant, also from a file, to make the point that the same
shortcode covers the whole family without four separate names.

{{< chart type="doughnut" src="data/asset-breakdown.json" caption="Share of total asset payload by category for a representative post." >}}

## Animate, one preset at a time

The `animate` shortcode wraps a region of content in a div that a small
Motion One driver animates into view as the reader scrolls past. There are
five presets. Each one is shown in turn below, with a one-sentence
explanation of where it earns its keep.

{{< animate preset="fade" >}}
The `fade` preset is the gentlest of the five. The wrapped content starts
at opacity zero and fades in once it enters the viewport. Use this for
content you do not want to call attention to but still want to feel alive.
{{< /animate >}}

{{< animate preset="slide-up" >}}
The `slide-up` preset slides the content in from below as well as fading
it in. Use this for section headings, summary paragraphs, and anything else
where you want the reader's eye to land on the block as the page scrolls.
{{< /animate >}}

{{< animate preset="slide-left" >}}
The `slide-left` preset slides the content in from the right. Use this
sparingly, mostly for asides or pull quotes that sit alongside the main
column.
{{< /animate >}}

{{< animate preset="scale" >}}
The `scale` preset starts the content at slightly less than full size and
scales it up as it fades in. Reserve this for the one element on the page
that needs to feel like it lands with weight. A hero image. A final result.
{{< /animate >}}

{{< animate preset="custom" keyframes="opacity:0,1; transform:translateY(40px),translateY(0)" >}}
The `custom` preset is the escape hatch. Pass a `keyframes` argument and
the driver feeds it directly into Motion One. Anything you can express as
a Motion One keyframes spec works here. Use this when none of the four
named presets fit and you have a reason to invent a new one.
{{< /animate >}}

## A playable Rust playground iframe

The `playable` shortcode currently supports one backend, `rust-playground`,
which embeds the official Rust playground as a sandboxed iframe. Pass a
`gist` argument to preload a snippet. The gist is symbolic here; in a real
post it would be the ID of a gist you control.

{{< playable id="rust-playground" gist="d4e5f6a7b8c9" >}}

The iframe is `loading="lazy"` and `sandbox="allow-scripts allow-same-origin allow-forms"`,
so the cost is only paid when the reader scrolls it into view, and the
iframe cannot pop dialogs or navigate the parent page.

## Embeds, three flavours

The `embed` shortcode classifies its `url` argument and falls back to a
plain link if it does not recognise the host. Three examples follow: a
YouTube video, a Twitter post, and an arbitrary URL.

A YouTube embed. The renderer extracts the video ID and emits an iframe
pointed at the no-cookie domain.

{{< embed url="https://www.youtube.com/watch?v=dQw4w9WgXcQ" >}}

A Twitter post. The renderer emits a `blockquote` that the official Twitter
widget script (loaded by the page template, not by the shortcode itself)
upgrades to a full embed on the client.

{{< embed url="https://twitter.com/rustlang/status/1234567890123456789" >}}

An arbitrary URL that the classifier does not recognise. The renderer falls
back to a plain anchor tag, which keeps the link intact without pretending
to know how to embed it.

{{< embed url="https://blog.rust-lang.org/2025/01/09/Rust-1.84.0/" >}}

The fallback is deliberate. It is better to render a working link than to
guess wrong about how a third-party page should be embedded. Adding a new
provider means extending the `classify` function in `crates/shortcodes/src/embed.rs`
and writing a test for the new URL shape.

## What was not covered

Three things were deliberately left out of this tour. The first is the
frontmatter shape itself, which is documented in the README. The second is
the asset manifest format that the renderer emits alongside the HTML, which
is also in the README. The third is the public HTTP layer, which is not yet
implemented; for now the CLI renderer is the only consumer.

With the seven shortcodes above, you can write any post that the engine is
designed for. If you find yourself wanting a block that does not exist,
the right move is to add it: implement the `Shortcode` trait in
`crates/shortcodes/src/`, register it in `default_registry()`, and write a
unit test. The render pipeline will pick it up automatically.
