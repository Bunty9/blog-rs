---
title: Bare-Metal Firmware and Real-Time Systems Development
subtitle: ~
tags: [rust, level-4, embedded]
series: rust-level-4
series_order: 1
status: draft
---

{{< callout type="info" >}}
Operating within a bare-metal environment requires systems engineers to construct runtimes entirely within a no_std compilation layout This restriction decouples the binary from the Rust standard library (std), linking instead directly against the core library (core) This layout eliminates operating system abstractions, meaning developers do not have access to dynamic memory allocation, standard multi-threading models, vector collections (Vec), stack overflow protection, or standard command-line argument processing To construct a functioning binary under these conditions, the application must declare the \#\!\[no_std\] and \#\!\[no_main\] attributes, while providing a custom non-returning panic handler 1:
{{< /callout >}}

Operating within a bare-metal environment requires systems engineers to construct runtimes entirely within a no_std compilation layout This restriction decouples the binary from the Rust standard library (std), linking instead directly against the core library (core) This layout eliminates operating system abstractions, meaning developers do not have access to dynamic memory allocation, standard multi-threading models, vector collections (Vec), stack overflow protection, or standard command-line argument processing To construct a functioning binary under these conditions, the application must declare the \#\!\[no_std\] and \#\!\[no_main\] attributes, while providing a custom non-returning panic handler 1:

{{< code lang="rust" playground="true" >}}
#[panic_handler]  
fn panic(_info: \&PanicInfo) ->! {  
 loop {}  
}
{{< /code >}}

Initializing microcontrollers, such as the ARM Cortex-M4, requires establishing memory and execution boundaries before jumping to user application logic The boot sequence requires a hardware vector table mapped precisely to memory address 0x00000000 This table configures the initial stack pointer and specifies the memory addresses for critical fault and interrupt handlers, including the reset handler, Non-Maskable Interrupts (NMI), and hard faults  
Once these registers are mapped, the runtime initializes global variables by clearing the .bss section to zero in physical memory and copying the initial values of the .data section from flash to SRAM With memory sections fully prepared, the initialization code executes a direct jump to the main application entry point  
To manage task execution and hardware events on the microcontroller, developers select an appropriate concurrency model The chosen model dictates how interrupts, CPU cycles, and shared resources are handled

<!-- TODO: chart? -->
| Concurrency Paradigm                                | Architectural Mechanism                                                                                              | Safety & Deadlock Guarantees                                                                     | Inter-Process Compatibility                                                         |
| :-------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------- |
| **Real-Time Interrupt-driven Concurrency (RTIC)** 4 | Bumps the preemption priority of a task above any other task sharing a locked mutex until the resource is released | Statically guarantees deadlock-free execution at compile time with zero runtime mutex overhead | Native Rust task priority mapping; primarily targeted at standalone MCU platforms |
| **Embassy Async Executor** 2                        | Implements a cooperative run-to-completion executor that polls active futures                                      | Enforces asynchronous memory safety through compiler-validated future state machines           | Integrates with legacy C codebases via Foreign Function Interface (FFI) bindings  |
| **Tock OS Kernel** 4                                | Employs system sandboxing to let multiple isolated applications share hardware peripherals                         | Isolates application crashes at the process level to prevent system-wide panic propagation     | Supports multi-language application execution (Rust, C, and assembly)             |

When opting for cooperative multitasking via the Embassy framework, the application's lifecycle is governed by three architectural pillars: the async executor, the Hardware Abstraction Layer (HAL), and the async peripheral drivers The executor is a no_std runtime that schedules tasks without relying on dynamic allocations, running a continuous polling loop to resolve ready futures Task registration and execution are initiated using macro-driven spawners 2:

{{< code lang="rust" playground="true" >}}
#[embassy::task]  
async fn sensor_reader(mut rx: UARTRx, mut tx: UARTTx) {  
 loop {  
 let data \= rx.read().await;  
 process_data(data);  
 tx.write_all(\&response).await;  
 }  
}
{{< /code >}}

This task spawning macro (spawner.spawn()) registers the task's future with the executor When a task awaits an asynchronous resource, it yields execution, allowing the executor to poll other ready tasks The runtime uses a dedicated timer queue and interrupt controller to wake sleeping tasks when their associated hardware events complete  
When safety-critical deployment is required, the underlying Rust toolchain can be validated using the Ferrocene Project Ferrocene, developed in collaboration with AdaCore, qualifies the Rust compiler and toolchain for high-integrity environments, positioning Rust as a robust alternative to MISRA C or Ada/SPARK in safety-critical systems

<!-- TODO: chart? -->
| Safety Standard | Target Domain               | Certification Level |
| :-------------- | :-------------------------- | :------------------ |
| **ISO 26262** 4 | Automotive Embedded Systems | ASIL-D 4            |
| **IEC 61508** 4 | Industrial Control Software | SIL 3-4 4           |
| **IEC 62304** 4 | Medical Devices & Software  | Class C 4           |

To write, analyze, and deploy bare-metal firmware, developers rely on a specialized systems tooling ecosystem This ecosystem spans register-level generation, stack layout optimization, and hardware-in-the-loop diagnostics

<!-- TODO: chart? -->
| Utility Tooling        | Technical Operation                                                              | Diagnostic and Safety Value                                               |
| :--------------------- | :------------------------------------------------------------------------------- | :------------------------------------------------------------------------ |
| xargo / cargo-xbuild 6 | Compiles and builds custom target-specific core runtimes                       | Enables custom compilation flags for non-standard target architectures  |
| svd2rust 6             | Generates type-safe register mappings from System View Description (SVD) files | Eliminates manual register mapping bugs and prevents invalid bit writes |
| edc2svd 6              | Translates PIC32-specific EDC files into standard SVD layouts                  | Extends type-safe register mappings to legacy Microchip architectures   |
| flip-link 6            | Flips the standard program layout, placing the stack below the data section    | Provides physical stack overflow protection without an MMU              |
| cargo-call-stack 6     | Statically analyzes execution paths to calculate worst-case stack usage        | Prevents stack collisions and memory corruption at compile time         |
| probe-rs 6             | Direct debugging toolkit communicating via JTAG/SWD protocol                   | Enables real-time target flashing, GDB hosting, and RTT monitoring      |
| embedded-test 6        | Test harness orchestrating unit, integration, and async tests on target        | Automates hardware-in-the-loop validation of embedded logic             |
| defmt 6                | Highly compressed, deferred logging framework for MCUs                         | Minimizes execution overhead and logging footprint in flash             |

## **Domain 2: High-Performance Network Proxies and Asynchronous Protocols**
