use crate::felib_getvalue;
use crate::FELibReturn;
use log::info;

const DIGITIZER_PARAMS: &[&str] = &[
    "CupVer",
    "FPGA_FwVer",
    "FwType",
    "ModelCode",
    "PBCode",
    "ModelName",
    "FormFactor",
    "FamilyCode",
    "SerialNum",
    "PCBrev_MB",
    "PCBrev_PB",
    "License",
    "LicenseStatus",
    "LicenseRemainingTime",
    "NumCh",
    "ADC_Nbit",
    "ADC_SamplRate",
    "InputRange",
    "InputType",
    "Zin",
    "ClockSource",
    "EnClockOutFP",
    "SFPLinkPresence",
    "SFPLinkActive",
    "SFPLinkProtocol",
    "IPAddress",
    "Netmask",
    "Gateway",
    "StartSource",
    "AcqTriggerSource",
    "TstampResetSource",
    "TrgOutMode",
    "GPIOMode",
    "BusyInSource",
    "SyncOutMode",
    "VetoSource",
    "VetoWidth",
    "VetoPolarity",
    "RunDelay",
    "EnTriggerOverlap",
    "EnAutoDisarmAcq",
    "EnMultiWindowRun",
    "PauseTimeStamp",
    "TriggerIDMode",
    "AcquisitionStatus",
    "VolatileClockOutDelay",
    "PermanentClockOutDelay",
    "RecordLengthS",
    "RecordLengthT",
    "MaxRawDataSize",
    "PreTriggerS",
    "PreTriggerT",
    "TriggerDelayS",
    "TriggerDelayT",
    "WaveDataSource",
    "TestPulsePeriod",
    "TestPulseWidth",
    "TestPulseLowLevel",
    "TestPulseHighLevel",
    "RealtimeMonitor",
    "DeadtimeMonitor",
    "LivetimeMonitor",
    "TriggerCnt",
    "LostTriggerCnt",
    "IOlevel",
    "TempSensAirIn",
    "TempSensAirOut",
    "TempSensCore",
    "TempSensFirstADC",
    "TempSensLastADC",
    "TempSensHottestADC",
    "TempSensADC0",
    "TempSensADC1",
    "TempSensADC2",
    "TempSensADC3",
    "TempSensADC4",
    "TempSensADC5",
    "TempSensADC6",
    "TempSensADC7",
    "TempSensDCDC",
    "VInSensDCDC",
    "VOutSensDCDC",
    "IOutSensDCDC",
    "FreqSensCore",
    "DutyCycleSensDCDC",
    "SpeedSensFan1",
    "SpeedSensFan2",
    "ErrorFlagMask",
    "ErrorFlagDataMask",
    "ErrorFlags",
    "BoardReady",
    "ITLAMainLogic",
    "ITLBMainLogic",
    "ITLAMajorityLev",
    "ITLBMajorityLev",
    "ITLAPairLogic",
    "ITLBPairLogic",
    "ITLAPolarity",
    "ITLBPolarity",
    "ITLAMask",
    "ITLBMask",
    "ITLConnect",
    "ITLAGateWidth",
    "ITLBGateWidth",
    "ITLAEnRetrigger",
    "ITLBEnRetrigger",
    "LVDSMode",
    "LVDSDirection",
    "LVDSIOReg",
    "LVDSTrgMask",
    "DACoutMode",
    "DACoutStaticLevel",
    "DACoutChSelect",
    "EnOffsetCalibration",
    "DecimationFactor",
    "EnChSuppr",
];

/// List of channel-level parameters (Level: CH) to read for each channel of a digitizer.
const CHANNEL_PARAMS: &[&str] = &[
    "ChEnable",
    "SelfTrgRate",
    "ChStatus",
    "DCOffset",
    "SignalOffset",
    "GainFactor",
    "ADCToVolts",
    "TriggerThr",
    "TriggerThrMode",
    "SelfTriggerEdge",
    "SelfTriggerWidth",
    "SamplesOverThreshold",
    "OverThresholdVetoWidth",
    "ChSupprThr",
    "ChSupprSamplesOverThreshold",
];

pub fn log_all(boards: &[(usize, u64)]) {
    for &(board_id, handle) in boards {
        let mut param_log = String::new();

        for &param in DIGITIZER_PARAMS {
            let path = format!("/par/{}", param);
            if let Ok(value) = felib_getvalue(handle, &path) {
                param_log.push_str(&format!("{}: {}\n", param, value));
            }
        }

        if let Ok(numch_str) = felib_getvalue(handle, "/par/NumCh") {
            if let Ok(total_ch) = numch_str.trim().parse::<usize>() {
                let groups = (total_ch + 3) / 4; // 4 channels per group
                for group in 0..groups {
                    let ch_index = group * 4;
                    let path = format!("/ch/{}/par/InputDelay", ch_index);
                    if let Ok(val) = felib_getvalue(handle, &path) {
                        param_log.push_str(&format!("InputDelay(group{}): {}\n", group, val));
                    }
                }
            }
        }

        let num_channels = if let Ok(n) = felib_getvalue(handle, "/par/NumCh")
            .and_then(|s| s.trim().parse().map_err(|_| FELibReturn::Generic))
        {
            n
        } else {
            0
        };

        if num_channels > 0 {
            for &ch_param in CHANNEL_PARAMS {
                for ch in 0..num_channels {
                    let path = format!("/ch/{}/par/{}", ch, ch_param);
                    match felib_getvalue(handle, &path) {
                        Ok(val) => {
                            param_log.push_str(&format!("{}[{}]: {}\n", ch_param, ch, val));
                        }
                        Err(_) => {
                            continue;
                        }
                    }
                }
            }
        }

        if param_log.ends_with('\n') {
            param_log.pop();
        }
        info!("Digitizer {} parameters:\n{}", board_id, param_log);
    }
}
