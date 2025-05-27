use confique::Config;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Config, Debug, Clone)]
pub struct Conf {
    #[config(nested)]
    pub run_settings: RunSettings,
    #[config(nested)]
    pub board_settings: BoardSettings,
    #[config(nested)]
    pub sync_settings: SyncSettings,
}

#[derive(Config, Debug, Clone)]
pub struct RunSettings {
    pub boards: Vec<String>,
    pub run_duration: u64,
    pub output_dir: String,
    pub campaign_num: usize,
    #[config(default = 5)]
    pub blosc_threads: u8,
    #[config(default = 2)]
    pub compression_level: u8,
    pub zs_level: f64,
    pub zs_threshold: f64,
    pub zs_edge: ZeroSuppressionEdge,
    pub zs_samples: isize,
}

#[derive(Config, Debug, Clone)]
pub struct BoardSettings {
    pub common: CommonSettings,
    pub boards: Vec<PerBoardSettings>,
}

#[derive(Deserialize, Config, Debug, Clone)]
pub struct CommonSettings {
    pub record_len: usize,
    pub pre_trig_len: usize,
}

#[derive(Deserialize, Config, Debug, Clone)]
pub struct PerBoardSettings {
    pub en_chans: ChannelConfig,
    pub trig_source: String,
    pub dc_offset: DCOffsetConfig,
    pub io_level: String,
    pub test_pulse_period: usize,
    pub test_pulse_width: usize,
    pub test_pulse_low: usize,
    pub test_pulse_high: usize,
    pub trig_thr: TriggerThr,
    pub trig_thr_mode: TriggerThrMode,
    pub trig_edge: TriggerEdge,
    pub samples_over_thr: SamplesOverThr,
    pub itl_logic: String,
    pub itl_majority_level: u8,
    pub itl_pair_logic: String,
    pub itl_polarity: String,
    pub itl_gatewidth: usize,
    pub itl_connect: ITLConnect,
    pub itl_retrig: String,
}

#[derive(Config, Debug, Clone)]
pub struct SyncSettings {
    pub boards: Vec<PerBoardSync>,
}

#[derive(Deserialize, Config, Debug, Clone)]
pub struct PerBoardSync {
    pub clock_src: String,
    pub sync_out: String,
    pub start_source: String,
    pub clock_out_fp: String,
    pub trig_out: String,
    pub auto_disarm: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ChannelConfig {
    All(bool),
    List(Vec<u32>),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum DCOffsetConfig {
    Global(f64),
    PerChannel(HashMap<String, f64>),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum TriggerThr {
    Global(isize),
    PerChannel(HashMap<String, isize>),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum TriggerThrMode {
    Global(String),
    PerChannel(HashMap<String, String>),
}

#[derive(Deserialize, Clone, Debug)]
pub enum TriggerEdge {
    Fall,
    Rise,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum SamplesOverThr {
    Global(usize),
    PerChannel(HashMap<String, usize>),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum ITLConnect {
    Global(String),
    PerChannel(HashMap<String, String>),
}

#[derive(Deserialize, Clone, Debug, Copy)]
pub enum ZeroSuppressionEdge {
    Fall,
    Rise,
}
