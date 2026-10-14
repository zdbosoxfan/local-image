//! DJI's XMP namespace, as documented by ExifTool's DJI XMP table.
//! Source scalar properties are retained verbatim for export, including calibration and RTK fields.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DJI_NS: &str = "http://www.dji.com/drone-dji/1.0/";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DjiMetadata {
    /// Metres; DJI's absolute and takeoff-relative altitude are distinct datums.
    pub absolute_altitude: Option<f64>,
    pub relative_altitude: Option<f64>,
    /// Degrees as recorded by the camera, without reinterpreting the reference frame.
    pub gimbal_yaw: Option<f64>,
    pub gimbal_pitch: Option<f64>,
    pub gimbal_roll: Option<f64>,
    pub flight_yaw: Option<f64>,
    pub flight_pitch: Option<f64>,
    pub flight_roll: Option<f64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// DJI FlightX/Y/ZSpeed (metres per second), north/east/vertical components.
    pub speed_x: Option<f64>,
    pub speed_y: Option<f64>,
    pub speed_z: Option<f64>,
    /// Namespace-local names and original text; source-only, read-only interchange.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
}

impl DjiMetadata {
    pub(crate) fn from_properties(properties: &BTreeMap<String, Vec<String>>) -> Option<Self> {
        let number =
            |names: &[&str]| names.iter().find_map(|n| properties.get(&format!("drone-dji:{n}"))?.first().and_then(|v| crate::parse_number(v)));
        let source: BTreeMap<_, _> =
            properties.iter().filter_map(|(k, v)| Some((k.strip_prefix("drone-dji:")?.to_string(), v.first()?.clone()))).collect();
        if source.is_empty() {
            return None;
        }
        Some(Self {
            absolute_altitude: number(&["AbsoluteAltitude"]),
            relative_altitude: number(&["RelativeAltitude"]),
            gimbal_yaw: number(&["GimbalYawDegree"]),
            gimbal_pitch: number(&["GimbalPitchDegree"]),
            gimbal_roll: number(&["GimbalRollDegree"]),
            flight_yaw: number(&["FlightYawDegree"]),
            flight_pitch: number(&["FlightPitchDegree"]),
            flight_roll: number(&["FlightRollDegree"]),
            latitude: number(&["GpsLatitude", "GPSLatitude"]).filter(|v| (-90.0..=90.0).contains(v)),
            longitude: number(&["GpsLongitude", "GpsLongtitude", "GPSLongitude", "GPSLongtitude"]).filter(|v| (-180.0..=180.0).contains(v)),
            speed_x: number(&["FlightXSpeed", "SpeedX"]),
            speed_y: number(&["FlightYSpeed", "SpeedY"]),
            speed_z: number(&["FlightZSpeed", "SpeedZ"]),
            properties: source,
        })
    }

    pub(crate) fn interchange_properties(&self) -> BTreeMap<String, String> {
        let mut props = BTreeMap::new();
        for (name, value) in [
            ("AbsoluteAltitude", self.absolute_altitude),
            ("RelativeAltitude", self.relative_altitude),
            ("GimbalYawDegree", self.gimbal_yaw),
            ("GimbalPitchDegree", self.gimbal_pitch),
            ("GimbalRollDegree", self.gimbal_roll),
            ("FlightYawDegree", self.flight_yaw),
            ("FlightPitchDegree", self.flight_pitch),
            ("FlightRollDegree", self.flight_roll),
            ("GpsLatitude", self.latitude),
            ("GpsLongitude", self.longitude),
            ("FlightXSpeed", self.speed_x),
            ("FlightYSpeed", self.speed_y),
            ("FlightZSpeed", self.speed_z),
        ] {
            if let Some(v) = value.filter(|v| v.is_finite()) {
                props.insert(name.to_string(), v.to_string());
            }
        }
        // Preserve the original spelling for source aliases rather than adding duplicate fields.
        for (canonical, aliases) in [
            ("GpsLatitude", &["GpsLatitude", "GPSLatitude"][..]),
            ("GpsLongitude", &["GpsLongitude", "GpsLongtitude", "GPSLongitude", "GPSLongtitude"][..]),
            ("FlightXSpeed", &["FlightXSpeed", "SpeedX"][..]),
            ("FlightYSpeed", &["FlightYSpeed", "SpeedY"][..]),
            ("FlightZSpeed", &["FlightZSpeed", "SpeedZ"][..]),
        ] {
            if aliases.iter().any(|alias| self.properties.contains_key(*alias)) {
                props.remove(canonical);
            }
        }
        props.extend(self.properties.clone());
        props.retain(|name, _| {
            !name.is_empty()
                && name
                    .bytes()
                    .enumerate()
                    .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && (b.is_ascii_digit() || b == b'-' || b == b'.')))
        });
        props
    }
}
