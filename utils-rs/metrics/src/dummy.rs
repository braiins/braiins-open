use std::marker::PhantomData;

#[derive(Debug, Default)]
pub struct GenericCounter<T>(PhantomData<T>);

impl<T> GenericCounterVec<T> {
    pub fn with_label_values(&self, _labels: &[&str]) -> &Self {
        self
    }

    pub fn inc(&self) {}
    pub fn inc_by(&self, _val: T) {}
}
impl<T> GenericCounter<T> {
    pub fn inc(&self) {}
    pub fn inc_by(&self, _val: T) {}
}

#[derive(Debug, Default)]
pub struct GenericCounterVec<T>(PhantomData<T>);

pub type IntCounter = GenericCounter<u64>;
pub type IntCounterVec = GenericCounterVec<u64>;

#[derive(Debug)]
pub struct Histogram;

impl Histogram {
    pub fn inc(&self) {}
    pub fn observe(&self, _val: f64) {}
}

#[derive(Debug)]
pub struct HistogramVec;

impl HistogramVec {
    pub fn with_label_values(&self, _label_values: &[&str]) -> &Self {
        self
    }
    pub fn inc(&self) {}
    pub fn observe(&self, _val: f64) {}
}

#[derive(Debug)]
pub struct HistogramTimer;
#[derive(Debug, Default, Copy, Clone)]
pub struct MetricsRegistry;

impl MetricsRegistry {
    pub fn register_generic_counter<T: Default>(
        &self,
        _name: &str,
        _help: &str,
    ) -> GenericCounter<T> {
        GenericCounter::default()
    }

    pub fn register_generic_counter_vec<T: Default>(
        &self,
        _name: &str,
        _help: &str,
        _label_names: &[&str],
    ) -> GenericCounterVec<T> {
        GenericCounterVec::default()
    }

    pub fn register_histogram(&self, _name: &str, _help: &str) -> Histogram {
        Histogram
    }

    pub fn register_histogram_vec(
        &self,
        _name: &str,
        _help: &str,
        _label_names: &[&str],
    ) -> HistogramVec {
        HistogramVec
    }
    pub fn to_text(&self) -> crate::Result<(Vec<u8>, String)> {
        Ok((Default::default(), Default::default()))
    }
}
