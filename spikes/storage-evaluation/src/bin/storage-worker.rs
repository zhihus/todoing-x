use std::{env, process};
use todoing_storage_evaluation::{Fault, RedbStore, SqliteStore};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: storage-worker <write|crash> <sqlite|redb> <path> [count] [worker-id]");
        process::exit(2);
    }

    let mode = args[0].as_str();
    let backend = args[1].as_str();
    let path = &args[2];

    match (mode, backend) {
        ("write", "sqlite") => {
            let count = args.get(3).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
            let worker = args.get(4).map(String::as_str).unwrap_or("worker");
            let mut store = SqliteStore::open(path).unwrap_or_else(fail);
            for index in 0..count {
                store
                    .create_task(&format!("{worker}-{index}"))
                    .unwrap_or_else(fail);
            }
        }
        ("write", "redb") => {
            let count = args.get(3).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
            let worker = args.get(4).map(String::as_str).unwrap_or("worker");
            let store = RedbStore::open(path).unwrap_or_else(fail);
            for index in 0..count {
                store
                    .create_task(&format!("{worker}-{index}"))
                    .unwrap_or_else(fail);
            }
        }
        ("crash", "sqlite") => {
            let mut store = SqliteStore::open(path).unwrap_or_else(fail);
            let _ = store.create_task_with_fault("interrupted", Fault::ExitAfterTask);
        }
        ("crash", "redb") => {
            let store = RedbStore::open(path).unwrap_or_else(fail);
            let _ = store.create_task_with_fault("interrupted", Fault::ExitAfterTask);
        }
        _ => {
            eprintln!("unsupported mode/backend: {mode}/{backend}");
            process::exit(2);
        }
    }
}

fn fail<T>(error: String) -> T {
    eprintln!("{error}");
    process::exit(1);
}
