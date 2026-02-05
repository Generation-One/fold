//! Tracing layer that forwards log events to SSE for admin users.
//!
//! This layer captures all tracing events (info, warn, error, debug) and
//! broadcasts them via the EventBroadcaster for real-time log streaming.

use std::sync::Arc;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

use super::event_broadcaster::EventBroadcaster;

/// A tracing layer that forwards events to SSE.
pub struct SseTracingLayer {
    broadcaster: Arc<EventBroadcaster>,
}

impl SseTracingLayer {
    pub fn new(broadcaster: Arc<EventBroadcaster>) -> Self {
        Self { broadcaster }
    }
}

impl<S: Subscriber> Layer<S> for SseTracingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Convert tracing level to our log level string
        let level = match *event.metadata().level() {
            Level::ERROR => "error",
            Level::WARN => "warn",
            Level::INFO => "info",
            Level::DEBUG => "debug",
            Level::TRACE => "debug", // Treat trace as debug
        };

        // Extract fields from the event
        let mut message = String::new();
        let mut fields = Vec::new();
        let mut job_id: Option<String> = None;
        let mut project: Option<String> = None;

        // Use a visitor to extract fields
        struct FieldVisitor<'a> {
            message: &'a mut String,
            fields: &'a mut Vec<String>,
            job_id: &'a mut Option<String>,
            project: &'a mut Option<String>,
        }

        impl<'a> tracing::field::Visit for FieldVisitor<'a> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let val = format!("{:?}", value);
                // Remove surrounding quotes if present
                let clean_val = if val.starts_with('"') && val.ends_with('"') && val.len() > 2 {
                    val[1..val.len() - 1].to_string()
                } else {
                    val
                };

                match field.name() {
                    "message" => *self.message = clean_val,
                    "job_id" => *self.job_id = Some(clean_val),
                    "project" => *self.project = Some(clean_val),
                    _ => self.fields.push(format!("{}={}", field.name(), clean_val)),
                }
            }

            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                match field.name() {
                    "message" => *self.message = value.to_string(),
                    "job_id" => *self.job_id = Some(value.to_string()),
                    "project" => *self.project = Some(value.to_string()),
                    _ => self.fields.push(format!("{}={}", field.name(), value)),
                }
            }

            fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
                match field.name() {
                    "job_id" => *self.job_id = Some(value.to_string()),
                    _ => self.fields.push(format!("{}={}", field.name(), value)),
                }
            }

            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                self.fields.push(format!("{}={}", field.name(), value));
            }

            fn record_i128(&mut self, field: &tracing::field::Field, value: i128) {
                self.fields.push(format!("{}={}", field.name(), value));
            }

            fn record_u128(&mut self, field: &tracing::field::Field, value: u128) {
                self.fields.push(format!("{}={}", field.name(), value));
            }

            fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
                self.fields.push(format!("{}={}", field.name(), value));
            }

            fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
                self.fields.push(format!("{}={}", field.name(), value));
            }

            fn record_error(
                &mut self,
                field: &tracing::field::Field,
                value: &(dyn std::error::Error + 'static),
            ) {
                self.fields.push(format!("{}={}", field.name(), value));
            }
        }

        let mut visitor = FieldVisitor {
            message: &mut message,
            fields: &mut fields,
            job_id: &mut job_id,
            project: &mut project,
        };

        event.record(&mut visitor);

        // Build the full message: main message + all fields
        let full_message = if fields.is_empty() {
            if message.is_empty() {
                // No message and no fields - use target as message
                event.metadata().target().to_string()
            } else {
                message
            }
        } else {
            // Append fields to message
            if message.is_empty() {
                fields.join(", ")
            } else {
                format!("{}, {}", message, fields.join(", "))
            }
        };

        // Add target/module context to message for better identification
        let target = event.metadata().target();
        let formatted_message = if target.starts_with("fold") || target.starts_with("tower_http") {
            // Shorten common prefixes for cleaner logs
            let short_target = target
                .strip_prefix("fold_core::")
                .or_else(|| target.strip_prefix("fold::"))
                .or_else(|| target.strip_prefix("tower_http::"))
                .unwrap_or(target);
            format!("[{}] {}", short_target, full_message)
        } else {
            format!("[{}] {}", target, full_message)
        };

        // Emit to SSE - use job_id if available, otherwise "system"
        let job_id_str = job_id.as_deref().unwrap_or("system");
        let job_type = if job_id.is_some() { "job" } else { "system" };

        self.broadcaster.job_log(
            job_id_str,
            job_type,
            None,
            project.as_deref(),
            level,
            &formatted_message,
            None,
        );
    }
}
