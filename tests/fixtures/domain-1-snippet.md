---
title: Bare-Metal Rust - Cortex-M4 Runtimes from Scratch
subtitle: no_std, vector tables, and deadlock-free concurrency
tags: [rust, embedded, systems]
series: rust-level-4
series_order: 1
status: draft
---

{{< callout type="info" >}}
Bare-metal Rust drops the standard library entirely. You link against `core` and own the boot path yourself.
{{< /callout >}}

A bare-metal application must declare `no_std` + `no_main` and provide its own
non-returning panic handler.

{{< code lang="rust" playground="true" >}}
#![no_std]
#![no_main]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! { loop {} }
{{< /code >}}

The boot sequence configures the hardware vector table at `0x00000000`,
clears `.bss`, and copies `.data` from flash to SRAM before jumping to `main`.

{{< chart type="bar" src="data/preempt-cycles.json" caption="RTIC priority preemption - measured handoff in CPU cycles" >}}
