//! A command line client for A2A v1.0 agents.

use a2a_client::A2aClient;
use a2a_types::{
    CancelTaskRequest, GetTaskRequest, ListTasksRequest, Message, SendMessageConfiguration,
    SendMessageRequest, SendMessageResponse, StreamResponse, SubscribeToTaskRequest,
    TaskPushNotificationConfig,
};
use clap::{Parser, Subcommand};
use futures_util::StreamExt;

#[derive(Parser)]
#[command(name = "a2a", about = "Talk to an A2A v1.0 agent")]
struct Args {
    /// Base URL of the agent, used to fetch its Agent Card.
    #[arg(long, default_value = "http://127.0.0.1:9999")]
    agent: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch and print the Agent Card.
    Card,
    /// Print the extended Agent Card, which needs authentication on a real agent.
    ExtendedCard,
    /// Send a message and wait for the task to settle.
    Send {
        /// The message text.
        text: Vec<String>,
        /// Continue an existing task instead of starting one.
        #[arg(long)]
        task_id: Option<String>,
        /// Return as soon as the task exists, without waiting for it to settle.
        #[arg(long)]
        return_immediately: bool,
    },
    /// Send a message and print updates as they stream in.
    Stream {
        /// The message text.
        text: Vec<String>,
        /// Continue an existing task instead of starting one.
        #[arg(long)]
        task_id: Option<String>,
    },
    /// Attach to a running task's stream.
    Subscribe {
        /// The task to watch.
        task_id: String,
    },
    /// Fetch one task.
    Get {
        /// The task to fetch.
        task_id: String,
    },
    /// List this agent's tasks.
    List {
        /// Only tasks in this context.
        #[arg(long)]
        context_id: Option<String>,
        /// Page size.
        #[arg(long)]
        page_size: Option<i32>,
    },
    /// Cancel a task.
    Cancel {
        /// The task to cancel.
        task_id: String,
    },
    /// Register a push notification webhook for a task.
    Watch {
        /// The task to watch.
        task_id: String,
        /// The webhook URL to post task updates to.
        url: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let client = A2aClient::discover(&args.agent).await?;

    match args.command {
        Command::Card => match client.agent_card() {
            Some(card) => print_json(card)?,
            None => eprintln!("no agent card was discovered"),
        },
        Command::ExtendedCard => print_json(&client.get_extended_agent_card().await?)?,

        Command::Send {
            text,
            task_id,
            return_immediately,
        } => {
            let mut request = SendMessageRequest::new(message(text.join(" "), task_id));
            if return_immediately {
                request = request.with_configuration(SendMessageConfiguration {
                    return_immediately: true,
                    ..Default::default()
                });
            }
            match client.send_message(request).await? {
                SendMessageResponse::Task(task) => print_json(&task)?,
                SendMessageResponse::Message(message) => print_json(&message)?,
            }
        }

        Command::Stream { text, task_id } => {
            let request = SendMessageRequest::new(message(text.join(" "), task_id));
            let mut events = Box::pin(client.send_streaming_message(request).await?);
            while let Some(event) = events.next().await {
                print_event(&event?);
            }
        }

        Command::Subscribe { task_id } => {
            let mut events = Box::pin(
                client
                    .subscribe_to_task(SubscribeToTaskRequest::new(task_id))
                    .await?,
            );
            while let Some(event) = events.next().await {
                print_event(&event?);
            }
        }

        Command::Get { task_id } => {
            print_json(&client.get_task(GetTaskRequest::new(task_id)).await?)?
        }

        Command::List {
            context_id,
            page_size,
        } => {
            let response = client
                .list_tasks(ListTasksRequest {
                    context_id,
                    page_size,
                    ..Default::default()
                })
                .await?;
            print_json(&response)?;
        }

        Command::Cancel { task_id } => {
            print_json(&client.cancel_task(CancelTaskRequest::new(task_id)).await?)?
        }

        Command::Watch { task_id, url } => {
            let mut config = TaskPushNotificationConfig::new(url);
            config.task_id = Some(task_id);
            print_json(&client.create_push_notification_config(config).await?)?;
        }
    }

    Ok(())
}

fn message(text: String, task_id: Option<String>) -> Message {
    let mut message = Message::user_text(uuid::Uuid::new_v4().to_string(), text);
    message.task_id = task_id;
    message
}

/// Prints a stream event as one readable line, which is what a terminal wants from SSE.
fn print_event(event: &StreamResponse) {
    match event {
        StreamResponse::Task(task) => {
            println!("task {} [{:?}]", task.id, task.status.state);
        }
        StreamResponse::StatusUpdate(update) => {
            let text = update
                .status
                .message
                .as_ref()
                .map(|message| message.text())
                .unwrap_or_default();
            println!("  status {:?} {}", update.status.state, text);
        }
        StreamResponse::ArtifactUpdate(update) => {
            let text: String = update
                .artifact
                .parts
                .iter()
                .filter_map(a2a_types::Part::as_text)
                .collect();
            println!(
                "  artifact {} {}{}",
                update.artifact.artifact_id,
                text.trim_end(),
                if update.last_chunk { " (final)" } else { "" }
            );
        }
        StreamResponse::Message(message) => println!("  message {}", message.text()),
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
