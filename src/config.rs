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
    pub run_duration: u64,
    pub output_dir: String,
    pub campaign_num: usize,
}

#[derive(Config, Debug, Clone)]
pub struct BoardSettings {
    pub en_chans: ChannelConfig,
    pub record_len: usize,
    pub pre_trig_len: usize,
    pub trig_source: String,
    pub dc_offset: DCOffsetConfig,
}

#[derive(Config, Debug, Clone)]
pub struct SyncSettings {
    pub primary_clock_src: String,
    pub primary_sync_out: String,
    pub primary_start_source: String,
    pub primary_clock_out_fp: String,
    pub secondary_clock_src: String,
    pub secondary_sync_out: String,
    pub secondary_start_source: String,
    pub secondary_clock_out_fp: String,
    pub auto_disarm: String,
    pub trig_out: String,
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
