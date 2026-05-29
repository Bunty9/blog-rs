---
title: High-Throughput Storage Engines and Database Internals
subtitle: ~
tags: [rust, level-4, databases]
series: rust-level-4
series_order: 3
status: draft
---

{{< callout type="info" >}}
When building high-performance storage engines, developers must understand the architectural tradeoffs between Log-Structured Merge (LSM) trees and traditional B+ Trees LSM-trees are optimized for high-write-throughput workloads by converting random write operations into sequential disk writes
{{< /callout >}}

When building high-performance storage engines, developers must understand the architectural tradeoffs between Log-Structured Merge (LSM) trees and traditional B+ Trees LSM-trees are optimized for high-write-throughput workloads by converting random write operations into sequential disk writes

| Architectural Tradeoff Metric | Log-Structured Merge (LSM) Tree                                                     | Traditional B+ Tree                                                                 |
| :---------------------------- | :---------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------- |
| **Write Complexity & Path**   | **<!-- TODO: diagram? -->** sequential writes via memory buffers and append-only logs        | <!-- TODO: diagram? --> random in-place page modifications on disk                           |
| **Read Performance**          | Lower read performance; requires scanning multiple SSTables and Bloom Filters    | Higher read performance; single-path node traversal with high locality           |
| **Write Amplification**       | Higher write amplification due to ongoing background compaction                  | Lower write amplification; page updates are written directly to target locations |
| **Space Amplification**       | Higher space amplification from obsolete key versions and tombstone accumulation | Lower space amplification; old data is overwritten in-place                      |
| **Tail Latency Consistency**  | Higher tail latency variance; background compaction can cause CPU spikes         | More predictable tail latency; write paths avoid bulk background merges          |

To build a high-performance LSM-tree, engineers must design and coordinate several key data structures

                        Client Write Operations
                                   │
                                   ├───► Put(Key, Value) / Delete(Key)
                                   │
                                   ▼
                      ┌────────────────────────┐
                      │  Write-Ahead Log (WAL)  │ (Durability)
                      └────────────┬───────────┘
                                   │ (Sequential Append)
                                   ▼
                      ┌────────────────────────┐
                      │    Mutable MemTable    │ (In-Memory SkipList)
                      └────────────┬───────────┘
                                   │ (Size Threshold Reached)
                                   ▼
                      ┌────────────────────────┐
                      │   Immutable MemTable   │ (Frozen State)
                      └────────────┬───────────┘
                                   │ (Background Flush Process)
                                   ▼
                      ┌────────────────────────┐
                      │     SSTable File       │ (Sorted Disk Blocks)
                      └────────────────────────┘

When a write operation (put or delete) is executed, the engine first appends the transaction sequentially to an on-disk Write-Ahead Log (WAL) to guarantee durability across crashes Once logged, the transaction is applied to an in-memory mutable MemTable, typically implemented as a sorted SkipList to support fast concurrent reads and range scans  
When the active MemTable reaches its size threshold, it is frozen to become an immutable MemTable, and a new mutable MemTable is allocated to handle incoming writes A background task then flushes the immutable table to disk as a Sorted String Table (SSTable) file  
Because keys are sorted within each SSTable, search operations can use binary search on block indexes These searches are optimized by Bloom Filters, which allow the engine to determine if a key is absent without performing disk I/O Deleted keys are recorded using tombstone markers, which are purged during compaction

                  LSM Compaction & Range Partitioning

Level 0: (Overlapping Ranges)  
 │ │  
 ▼ ▼  
 ┌──────────────────────────────────────────────────┐  
 │ Compaction Merge-Sort Process │  
 └────────────────────────┬─────────────────────────┘  
 │  
 ▼  
 Level 1: (Strictly Partitioned)

As SSTables accumulate on disk, read performance can degrade The engine addresses this through compaction, which merges overlapping SSTables into consolidated, range-partitioned runs Compaction strategies are tailored to balance specific read, write, and space amplification trade-offs  
On startup, the engine reads the Manifest file—an append-only log that tracks the active SSTables, level mappings, and the WAL recovery sequence number—to reconstruct its state  
While many storage architectures rely on standard disk-bound LSM designs, alternative approaches have emerged

<!-- TODO: chart? -->
| Database System | Architectural Foundation                                                       | Compaction & Storage Model                                                                         |
| :-------------- | :----------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------- |
| **Sled** 11     | Lock-free B+ tree structured over a lock-free page cache and sequential log | Partial page fragments are scattered across the log and materialized using scatter-gather reads |
| **SlateDB** 13  | Object-storage optimized LSM-tree designed for cloud runtimes               | WAL, MemTables, SSTables, and Manifests are written directly to object storage targets          |
| **Fjall** 12    | Structured storage engine built on top of the primitive lsm-tree crate      | Combines a managed WAL with automatic table compaction and in-memory block caching              |

A key constraint of primitive LSM implementations, such as the core lsm-tree crate, is that they do not include a Write-Ahead Log by default, requiring the database layer (e.g., fjall) to manage write persistence and handle manual memtable flushes Furthermore, key sizes are restricted to <!-- TODO: diagram? --> bytes and values to <!-- TODO: diagram? --> bytes, with larger payloads causing performance degradation as keys scale  
For highly concurrent workloads, Sled’s lock-free page cache offers an alternative to typical B+ Trees Rather than writing whole pages in-place, Sled scatters partial page fragments across a continuous log When a page is read, the engine performs a concurrent scatter-gather operation across the log to reconstruct the active page state  
Sled’s ongoing integration with the Komora project and the Marble storage engine aims to reduce space and write amplification through several key refinements 11:

- **Node Memory Optimization**: Completely rewrites the memory layout of tree nodes to eliminate dynamic fragmentation and remove serialization overhead
- **Reactive Atomic Triggers**: Replaces standard database merge operations with reactive trigger functions, enabling atomic write batching while maintaining serializable consistency
- **Garbage Collection Refinement**: Consolidates page fragments inside the underlying Marble engine to reduce write amplification and reclaim disk space

To support concurrent transactions without blocking readers, storage engines implement Multi-Version Concurrency Control (MVCC) with snapshot isolation MVCC appends monotonic timestamps or sequence numbers to each key, allowing the engine to maintain multiple versions of a single key  
When a transaction begins, it is assigned a read snapshot matching the current global sequence number Read queries only see key versions with timestamps less than or equal to this snapshot sequence number, ensuring consistency Because writers append new versions with higher sequence numbers, concurrent readers can query the database without acquiring locks or blocking writing transactions

## **Domain 4: Decentralized State Transition Engines and Ledger Architectures**
