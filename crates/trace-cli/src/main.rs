use std::env;

use trace_server::TraceApi;

fn main() {
    let root = env::var("TRACE_ROOT").unwrap_or_else(|_| ".".to_string());
    let api = match TraceApi::from_root(&root) {
        Ok(api) => api,
        Err(error) => {
            eprintln!("failed to load TRACE events from root '{root}': {error}");
            std::process::exit(1);
        }
    };

    let mut args = env::args();
    let _binary = args.next();
    let command = args.next().unwrap_or_else(|| "tasks".to_string());

    match command.as_str() {
        "tasks" => {
            for task in api.get_tasks() {
                println!(
                    "{}\t{:?}\t{}",
                    task.task.task_id, task.status, task.task.title
                );
            }
        }
        "task" => {
            if let Some(task_id) = args.next() {
                match api.get_task(&task_id) {
                    Some(task) => println!(
                        "{}\t{:?}\tholder={}",
                        task.task.task_id,
                        task.status,
                        task.status_detail
                            .and_then(|detail| detail.holder)
                            .unwrap_or_else(|| "none".to_string())
                    ),
                    None => {
                        eprintln!("task not found: {task_id}");
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("usage: trace-cli task <TASK_ID>");
                std::process::exit(2);
            }
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("supported: tasks, task <TASK_ID>");
            std::process::exit(2);
        }
    }
}
