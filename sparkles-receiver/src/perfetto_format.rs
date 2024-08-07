use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfettoTraceFile {
    pub trace_events: Vec<PerfettoTraceEvent>,
    pub thread_names: HashMap<u64, String>,
}

impl PerfettoTraceFile {
    pub fn new() -> Self {
        Self {
            trace_events: vec![],
            thread_names: HashMap::new(),
        }
    }

    pub fn add_range_event(&mut self, name: String, event_id: usize, timestamp: u64, duration: u32) {
        self.trace_events.push(PerfettoTraceEvent::Range(RangeEvent::new(name, event_id, timestamp, duration)));
    }

    pub fn add_point_event(&mut self, name: String, event_id: usize, timestamp: u64) {
        self.trace_events.push(PerfettoTraceEvent::Point(PointEvent::new(name, event_id as usize, timestamp)));
    }

    pub fn set_thread_name(&mut self, event_id: usize, thread_name: String) {
        self.trace_events.push(PerfettoTraceEvent::ThreadName(ThreadNameMeta::new(event_id, thread_name)));
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum PerfettoTraceEvent {
    Range(RangeEvent),
    Point(PointEvent),
    ThreadName(ThreadNameMeta)
}


#[derive(Serialize, Deserialize)]
pub struct RangeEvent {
    pub name: String,
    pub cat: String,
    pub ph: String,
    pub ts: f64,
    pub dur: f64,
    pub tid: u64,
}

impl RangeEvent {
    pub fn new(name: String, event_id: usize, timestamp: u64, duration: u32) -> Self {
        Self {
            name,
            cat: "Range".to_string(),
            ph: "X".to_string(),
            ts: (timestamp as f64) / 1_000.0,
            dur: (duration as f64) / 1_000.0,
            tid: event_id as u64,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PointEvent {
    pub name: String,
    pub cat: String,
    pub ph: String,
    pub ts: f64,
    pub tid: u64,
}

impl PointEvent {
    pub fn new(name: String, event_id: usize, timestamp: u64) -> Self {
        Self {
            name,
            cat: "Point".to_string(),
            ph: "i".to_string(),
            ts: (timestamp as f64) / 1_000.0,
            tid: event_id as u64,
        }
    }
}


#[derive(Serialize, Deserialize)]
pub struct ThreadNameMeta {
    pub name: String,
    pub ph: String,
    pub tid: u64,
    pub args: HashMap<String, String>,
}

impl ThreadNameMeta {
    pub fn new(event_id: usize, thread_name: String) -> Self {
        Self {
            name: "thread_name".to_string(),
            ph: "M".to_string(),
            tid: event_id as u64,
            args: HashMap::from([("name".to_string(), thread_name)]),
        }
    }
}