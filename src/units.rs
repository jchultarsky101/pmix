//! Converting a measure to the unit everything is compared in.
//!
//! A design does not change when it is exported in different units, so
//! neither should its ids or a diff of it. Both readers state what a
//! file declares, and everything that compares or keys a number puts it
//! in the canonical unit first.
//!
//! Millimetres and degrees are the canonical units: they are what the
//! overwhelming majority of mechanical CAD states, so choosing them
//! leaves most files' numbers untouched.

/// The unit lengths are compared in.
pub const LENGTH: &str = "mm";

/// The unit angles are compared in.
pub const ANGLE: &str = "deg";

/// How many millimetres one `unit` is, or `None` when it is not a length
/// this knows.
pub fn millimetres(unit: &str) -> Option<f64> {
    Some(match unit.trim() {
        "nm" => 1e-6,
        "um" => 1e-3,
        "mm" => 1.0,
        "cm" => 10.0,
        "dm" => 100.0,
        "m" => 1000.0,
        "km" => 1e6,
        "mil" => 0.0254,
        "in" => 25.4,
        "ft" => 304.8,
        "yd" => 914.4,
        "mi" => 1_609_344.0,
        _ => return None,
    })
}

/// How many degrees one `unit` is, or `None` when it is not an angle
/// this knows.
pub fn degrees(unit: &str) -> Option<f64> {
    Some(match unit.trim() {
        "deg" | "°" => 1.0,
        "rad" => 180.0 / std::f64::consts::PI,
        "grad" | "gon" => 0.9,
        _ => return None,
    })
}

/// `value` stated in `unit`, put into the canonical unit for whatever it
/// measures, with that unit's name.
///
/// A unit this does not know is left alone, named as it was: a volume in
/// `mm3` or a mass in `kg` is still comparable with itself, and guessing
/// at a conversion would be worse than not converting.
pub fn canonical(value: f64, unit: &str) -> (f64, String) {
    if let Some(mm) = millimetres(unit) {
        return (value * mm, LENGTH.to_owned());
    }
    if let Some(deg) = degrees(unit) {
        return (value * deg, ANGLE.to_owned());
    }
    (value, unit.to_owned())
}

/// How many millimetres one unit of `declared` is, or 1 when the file
/// declared nothing or something unrecognised.
///
/// Used to put a model's coordinates into millimetres before they are
/// fingerprinted, so that the same design exported in inches and in
/// millimetres gives one set of ids.
pub fn scale_to_millimetres(declared: Option<&str>) -> f64 {
    declared.and_then(millimetres).unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_length_becomes_millimetres() {
        assert_eq!(canonical(1.0, "in"), (25.4, "mm".to_owned()));
        assert_eq!(canonical(2.5, "mm"), (2.5, "mm".to_owned()));
        assert_eq!(canonical(1.0, "m"), (1000.0, "mm".to_owned()));
        // The same length stated two ways is one number.
        let (a, ua) = canonical(1.0, "in");
        let (b, ub) = canonical(25.4, "mm");
        assert_eq!((a, ua), (b, ub));
    }

    #[test]
    fn an_angle_becomes_degrees() {
        let (v, u) = canonical(std::f64::consts::PI, "rad");
        assert!((v - 180.0).abs() < 1e-12, "{v}");
        assert_eq!(u, "deg");
        assert_eq!(canonical(90.0, "deg"), (90.0, "deg".to_owned()));
    }

    #[test]
    fn a_unit_with_no_conversion_is_left_as_it_stands() {
        // A volume, a mass, and a unit nobody declared.
        assert_eq!(canonical(4.0, "mm3"), (4.0, "mm3".to_owned()));
        assert_eq!(canonical(4.0, "kg"), (4.0, "kg".to_owned()));
        assert_eq!(canonical(4.0, ""), (4.0, String::new()));
    }

    #[test]
    fn a_model_scales_by_what_it_declared() {
        assert_eq!(scale_to_millimetres(Some("mm")), 1.0);
        assert_eq!(scale_to_millimetres(Some("in")), 25.4);
        // Nothing declared, or something unknown: leave the numbers be
        // rather than move them by a guess.
        assert_eq!(scale_to_millimetres(None), 1.0);
        assert_eq!(scale_to_millimetres(Some("furlong")), 1.0);
    }
}
