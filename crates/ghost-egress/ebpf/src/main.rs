//! eBPF cgroup egress filter program (Task 11.3).
//!
//! This is the eBPF program source that gets compiled and loaded by
//! `EbpfEgressPolicy`. It attaches to an agent's cgroup as a `CgroupSkb`
//! program and filters outbound connections by destination IP.
//!
//! # How it works
//!
//! 1. Userspace resolves allowed domains to IPs and populates the
//!    `ALLOWED_IPS` HashMap via Aya.
//! 2. This program intercepts `connect4`/`connect6` calls.
//! 3. It extracts the destination IP from the socket buffer.
//! 4. If the IP is in `ALLOWED_IPS` → allow (return 1).
//! 5. If not → drop (return 0) and emit a perf event for violation logging.
//!
//! # Build
//!
//! This program is compiled with `cargo xtask build-ebpf` using the
//! Aya build toolchain. The compiled bytecode is embedded in the
//! `EbpfEgressPolicy` at load time.
//!
//! # Requirements
//!
//! - Linux kernel 5.8+ (for CgroupSkb attach)
//! - `CAP_BPF` capability
//! - Aya runtime in userspace
//!
//! # Note
//!
//! This is a stub implementation. The actual eBPF program requires the
//! `aya-ebpf` crate and must be compiled with the BPF target
//! (`bpfel-unknown-none`). The structure below shows the intended
//! program layout.

// In production, this would use:
// #![no_std]
// #![no_main]
//
// use aya_ebpf::{
//     macros::{cgroup_skb, map},
//     maps::HashMap,
//     programs::SkBuffContext,
// };
// use aya_log_ebpf::info;

// /// Per-cgroup allowlist of destination IPs.
// ///
// /// Key: destination IPv4 address as u32 (network byte order).
// /// Value: 1 (allowed). Absence means blocked.
// ///
// /// Populated by userspace `EbpfEgressPolicy::apply()` after DNS resolution.
// /// Updated every 5 minutes by the periodic re-resolution task.
// #[map]
// static ALLOWED_IPS_V4: HashMap<u32, u8> = HashMap::with_max_entries(1024, 0);
//
// /// IPv6 allowlist.
// ///
// /// Key: destination IPv6 address as [u8; 16].
// /// Value: 1 (allowed).
// #[map]
// static ALLOWED_IPS_V6: HashMap<[u8; 16], u8> = HashMap::with_max_entries(1024, 0);
//
// /// Perf event buffer for violation logging.
// ///
// /// Userspace reads violation events from this buffer to increment
// /// the violation counter and potentially emit TriggerEvent::NetworkEgressViolation.
// #[map]
// static VIOLATIONS: aya_ebpf::maps::PerfEventArray<ViolationEvent> =
//     aya_ebpf::maps::PerfEventArray::new(0);
//
// /// Violation event sent to userspace via perf buffer.
// #[repr(C)]
// struct ViolationEvent {
//     /// Destination IP (v4 as u32, v6 would need separate struct).
//     dst_ip: u32,
//     /// Protocol (TCP=6, UDP=17).
//     protocol: u8,
//     /// Destination port.
//     dst_port: u16,
// }
//
// /// CgroupSkb egress filter — intercepts outbound connections.
// ///
// /// Attached to the agent's cgroup. Checks destination IP against
// /// the `ALLOWED_IPS_V4` / `ALLOWED_IPS_V6` maps.
// ///
// /// Returns:
// /// - 1: allow the packet
// /// - 0: drop the packet
// #[cgroup_skb]
// pub fn ghost_egress_filter(ctx: SkBuffContext) -> i32 {
//     match try_filter(ctx) {
//         Ok(ret) => ret,
//         Err(_) => 1, // On error, allow (fail-open for safety).
//     }
// }
//
// fn try_filter(ctx: SkBuffContext) -> Result<i32, i64> {
//     // 1. Parse IP header to extract destination address.
//     // 2. Check protocol version (IPv4 vs IPv6).
//     // 3. Look up destination in ALLOWED_IPS_V4 or ALLOWED_IPS_V6.
//     // 4. If found → return 1 (allow).
//     // 5. If not found → emit ViolationEvent to perf buffer, return 0 (drop).
//     Ok(1)
// }
//
// #[panic_handler]
// fn panic(_info: &core::panic::PanicInfo) -> ! {
//     unsafe { core::hint::unreachable_unchecked() }
// }

/// Stub main for non-eBPF builds. The actual eBPF program above is
/// compiled separately with the BPF target toolchain.
fn main() {
    eprintln!("This is the eBPF program source for ghost-egress.");
    eprintln!("It must be compiled with `cargo xtask build-ebpf` using the BPF target.");
    eprintln!("See crates/ghost-egress/ebpf/src/main.rs for the program layout.");
    std::process::exit(1);
}
