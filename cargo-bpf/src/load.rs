use crate::CommandError;
use crate::ebpf_io::PerfMessageStream;

use std::fs;
use std::path::PathBuf;
use redbpf::cpus;
use redbpf::{Module, PerfMap};
use redbpf::ProgramKind::*;
use hexdump::hexdump;
use futures::future::{self, Future, IntoFuture};
use futures::stream::Stream;
use tokio;

pub fn load(program: &PathBuf, interface: Option<&str>) -> Result<(), CommandError> {
    let data = fs::read(program)?;
    let mut module = Module::parse(&data).expect("failed to parse ELF data");
    for prog in module.programs.iter_mut() {
        prog.load(module.version, module.license.clone()).expect("failed to load program");
    } 

    if let Some(interface) = interface {
        for prog in module.programs.iter_mut().filter(|p| p.kind == XDP) {
            println!("Loaded: {}, {:?}", prog.name, prog.kind);
            prog.attach_xdp(interface).unwrap();
        }
    }

    for prog in module
        .programs
        .iter_mut()
        .filter(|p| p.kind == Kprobe || p.kind == Kretprobe)
    {
        prog.attach_probe()
            .expect(&format!("Failed to attach kprobe {}", prog.name));
        println!("Loaded: {}, {:?}", prog.name, prog.kind);
    }
    let online_cpus = cpus::get_online().unwrap();
    let mut futs = Vec::new();
    for m in module.maps.iter_mut().filter(|m| m.kind == 4) {
        for cpuid in online_cpus.iter() {
            let map = PerfMap::bind(m, -1, *cpuid, 16, -1, 0).unwrap();
            let stream = PerfMessageStream::new(
                m.name.clone(),
                map,
            );
            let fut = stream.for_each(|events| {
                for event in events {
                    println!("-- Event --");
                    hexdump(&event);
                }
                future::ok(())
            })
            .map_err(|e| ());
            futs.push(fut);
        }
    }

    let f = future::join_all(futs).map(|_| ());
    tokio::run(f);

    Ok(())
}