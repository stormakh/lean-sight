//! Exemplar sampling and framing: promlib `querydata/exemplar` (standard deviation sampler, no-op sampler,
//! label tracker, framer).
use super::frame::{Field, Frame, Labels, Meta, Values, labels_get};
use super::models::align_time_range;
use std::collections::BTreeMap;

/// One exemplar: a row of an exemplar frame.
#[derive(Debug, Clone)]
pub struct Exemplar {
    pub series_labels: Labels,
    /// Index of the source frame in the framer's `sources` (its label fields are columns 2..).
    pub source: usize,
    pub row_idx: usize,
    pub value: f64,
    /// Unix nanoseconds.
    pub timestamp: i64,
}

pub trait Sampler {
    fn add(&mut self, ex: Exemplar);
    fn set_step(&mut self, step: i64);
    fn sample(&mut self) -> Vec<Exemplar>;
    fn reset(&mut self);
}

/// `NoOpSampler`: every exemplar, sorted by time.
#[derive(Default)]
pub struct NoOpSampler {
    exemplars: Vec<Exemplar>,
}
impl Sampler for NoOpSampler {
    fn add(&mut self, ex: Exemplar) {
        self.exemplars.push(ex);
    }
    fn set_step(&mut self, _step: i64) {}
    fn sample(&mut self) -> Vec<Exemplar> {
        json::sort::sort_by(&mut self.exemplars, |a, b| a.timestamp.cmp(&b.timestamp));
        self.exemplars.clone()
    }
    fn reset(&mut self) {
        self.exemplars.clear();
    }
}

/// `StandardDeviationSampler`: per step bucket, the highest value and the values more than two standard
/// deviations below the previous kept one.
#[derive(Default)]
pub struct StdDevSampler {
    step: i64,
    buckets: BTreeMap<i64, Vec<Exemplar>>,
    count: u64,
    mean: f64,
    m2: f64,
}
impl StdDevSampler {
    fn std_dev(&self) -> f64 {
        if self.count < 2 {
            return 0.0;
        }
        (self.m2 / (self.count - 1) as f64).sqrt()
    }
}
impl Sampler for StdDevSampler {
    fn set_step(&mut self, step: i64) {
        self.step = step;
    }
    fn add(&mut self, ex: Exemplar) {
        let b = align_time_range(ex.timestamp, self.step, 0);
        self.count += 1;
        let delta = ex.value - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = ex.value - self.mean;
        self.m2 += delta * delta2;
        self.buckets.entry(b).or_default().push(ex);
    }
    fn sample(&mut self) -> Vec<Exemplar> {
        let sd = self.std_dev();
        let mut out: Vec<Exemplar> = Vec::new();
        for (_, b) in self.buckets.iter_mut() {
            json::sort::sort_by(b, |x, y| y.value.partial_cmp(&x.value).unwrap_or(std::cmp::Ordering::Equal));
            let mut sampled: Vec<Exemplar> = Vec::new();
            for ex in b.iter() {
                match sampled.last() {
                    None => sampled.push(ex.clone()),
                    Some(prev) => {
                        if sd != 0.0 && prev.value - ex.value > sd * 2.0 {
                            sampled.push(ex.clone());
                        }
                    }
                }
            }
            out.extend(sampled);
        }
        json::sort::sort_by(&mut out, |a, b| a.timestamp.cmp(&b.timestamp));
        out
    }
    fn reset(&mut self) {
        self.step = 0;
        self.buckets.clear();
        self.count = 0;
        self.mean = 0.0;
        self.m2 = 0.0;
    }
}

/// `LabelTracker` + `Framer`: collects the non-exemplar frames as they are, and turns the sampled exemplars into
/// one frame named "exemplar" appended last.
pub struct Framer {
    pub frames: Vec<Frame>,
    /// The exemplar frames the samples point into.
    pub sources: Vec<Frame>,
    names: Vec<String>,
    pub meta: Option<Meta>,
    pub ref_id: String,
}

impl Default for Framer {
    fn default() -> Self {
        Framer::new()
    }
}

impl Framer {
    pub fn new() -> Framer {
        Framer { frames: Vec::new(), sources: Vec::new(), names: Vec::new(), meta: None, ref_id: String::new() }
    }
    pub fn add_frame(&mut self, f: Frame) {
        self.frames.push(f);
    }
    /// Label tracker `Add`.
    pub fn track_labels(&mut self, l: &Labels) {
        for (k, _) in l.iter() {
            self.track_name(k);
        }
    }
    pub fn track_name(&mut self, n: &str) {
        if let Err(i) = self.names.binary_search_by(|x| x.as_str().cmp(n)) {
            self.names.insert(i, n.to_string());
        }
    }

    /// `Framer.Frames`.
    pub fn frames(mut self, sampler: &mut dyn Sampler) -> Vec<Frame> {
        let ex = sampler.sample();
        sampler.reset();
        if ex.is_empty() {
            return self.frames;
        }
        let mut times = Vec::with_capacity(ex.len());
        let mut values = Vec::with_capacity(ex.len());
        let mut cols: Vec<Vec<String>> = vec![Vec::with_capacity(ex.len()); self.names.len()];
        for e in ex.iter() {
            times.push(e.timestamp);
            values.push(e.value);
            for (i, n) in self.names.iter().enumerate() {
                // series label first; a label field of the source frame overrides it (every tracked name is set)
                let mut v = labels_get(&e.series_labels, n).unwrap_or("").to_string();
                if let Some(src) = self.sources.get(e.source) {
                    for f in src.fields.iter().skip(2) {
                        if &f.name == n {
                            if let Values::Str(s) = &f.values {
                                v = s.get(e.row_idx).cloned().unwrap_or_default();
                            }
                        }
                    }
                }
                cols[i].push(v);
            }
        }
        let mut fields = vec![Field::new("Time", Values::Time(times)), Field::new("Value", Values::F64(values))];
        for (i, n) in self.names.iter().enumerate() {
            fields.push(Field::new(n, Values::Str(std::mem::take(&mut cols[i]))));
        }
        let mut f = Frame::with_fields("exemplar", fields);
        f.ref_id = self.ref_id.clone();
        f.meta = self.meta.take();
        self.frames.push(f);
        self.frames
    }
}
