//! The cost of one measurement pass (plan 3.2): a process snapshot, CPU
//! figures and an ownership walk. The budget is 0.1% of the machine at the
//! 10 s cadence the host uses while its window is closed.

use std::time::Instant;

use n8tray_core::verify::{self, ProcessTable, Reported};
use n8tray_platform::process::{Processes, logical_cpus};

#[test]
fn a_measurement_pass_fits_the_budget() {
    let me = std::process::id();
    let mut processes = Processes::new();
    let cpu_of_me = |p: &mut Processes| {
        p.snapshot()
            .unwrap()
            .processes
            .iter()
            .find(|x| x.pid == me)
            .unwrap()
            .cpu_time_ms
    };
    let mut table = ProcessTable::new(processes.snapshot().unwrap());
    let passes = 30;
    let cpu_before = cpu_of_me(&mut processes);
    let wall = Instant::now();
    for _ in 0..passes {
        let next =
            ProcessTable::new(processes.snapshot().unwrap()).with_cpu_from(&table, logical_cpus());
        // Ten tracked processes' worth of ownership checks.
        let reported: Vec<Reported> = next
            .iter()
            .take(10)
            .map(|p| Reported {
                pid: p.pid,
                started_at: p.started_at,
            })
            .collect();
        let _ = verify::attribute(
            &next,
            Some((me, next.get(me).unwrap().started_at)),
            &[],
            &reported,
            |_| false,
        );
        table = next;
    }
    let cpu_ms = (cpu_of_me(&mut processes) - cpu_before) as f64 / passes as f64;
    let wall_ms = wall.elapsed().as_secs_f64() * 1000.0 / passes as f64;
    // Share of the whole machine at one pass every 10 s.
    let share = cpu_ms / (10_000.0 * logical_cpus() as f64) * 100.0;
    println!(
        "one pass: {cpu_ms:.2} ms CPU, {wall_ms:.2} ms wall, {} processes; at 10 s: {share:.4}% of {} CPUs",
        table.len(),
        logical_cpus()
    );
    // Debug builds are several times slower than the release the tray ships.
    let budget = if cfg!(debug_assertions) { 0.5 } else { 0.1 };
    assert!(share < budget, "{share:.4}% is over the {budget}% budget");
}
