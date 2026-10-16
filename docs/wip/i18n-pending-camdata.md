# Camera data strings pending translation

Camera-measured WB names from the whole pinned darktable `wb_presets.json` now appear in the
existing WB dropdown through `i18n::tr`. Translate the unique zero-tuning preset names as the
lowest-priority follow-up; preserve upstream camera/model names as data. New command error:
`unknown camera WB preset \`NAME\``. No new literal UI label was introduced.

Unique camera preset strings exposed by the zero-tuning menu:

- 10000K
- 2500K
- 2550K
- 2560K
- 2630K
- 2650K
- 2700K
- 2780K
- 2800K
- 2850K
- 2860K
- 2940K
- 2950K
- 3000K
- 3030K
- 3100K
- 3130K
- 3200K
- 3230K
- 3300K
- 3330K
- 3400K
- 3450K
- 3570K
- 3600K
- 3700K
- 3800K
- 3850K
- 3900K
- 4000K
- 4170K
- 4200K
- 4300K
- 4350K
- 4500K
- 4550K
- 4760K
- 4800K
- 5000K
- 5260K
- 5300K
- 5500K
- 5560K
- 5600K
- 5880K
- 5900K
- 6000K
- 6250K
- 6300K
- 6500K
- 6600K
- 6670K
- 6700K
- 7100K
- 7140K
- 7500K
- 7690K
- 7700K
- 8300K
- 8330K
- 8500K
- 9090K
- 9100K
- Black & White
- Cloudy
- Cool White Fluorescent
- D55
- Day White Fluorescent
- Daylight
- Daylight Fluorescent
- Direct Sunlight
- Evening Sun
- Flash
- Flash (auto mode)
- Flash WB
- Fluorescent
- Fluorescent High
- High Temp. Mercury-Vapor
- High Temp. Mercury-Vapor Fluorescent
- Incandescent
- Incandescent Warm
- Neutral Fluorescent
- Shade
- Shadow
- Sodium-Vapor Fluorescent
- Sunny
- Tungsten
- Underwater
- Warm White Fluorescent
- White Fluorescent

New DNG export error: `per-channel vendor whites require linearized DNG export`.
