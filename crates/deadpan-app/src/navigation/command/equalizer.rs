use super::Entry;
use deadpan_core::EqualizerRecipe;

const USAGE: &str = "Use :eq to edit, :eq bass=3dB middle=-6dB treble=0dB [low-cut=80Hz] [high-cut=12000Hz], or :eq off. All three bands are required; omitted cuts are off.";

pub(super) fn parse<'a>(words: impl Iterator<Item = &'a str>) -> Result<Entry, String> {
    let mut words = words.peekable();
    let Some(first) = words.peek().copied() else {
        return Ok(Entry::EqualizerEdit);
    };
    if first == "off" {
        words.next();
        return if words.next().is_none() {
            Ok(Entry::Equalizer(None))
        } else {
            Err(USAGE.into())
        };
    }
    let mut gains = [None; 3];
    let mut low = None;
    let mut high = None;
    for word in words {
        let (key, value) = word.split_once('=').ok_or(USAGE)?;
        match key {
            "bass" | "middle" | "treble" => {
                let index = match key {
                    "bass" => 0,
                    "middle" => 1,
                    _ => 2,
                };
                if gains[index].is_some() {
                    return Err("Give each EQ band once.".into());
                }
                let db = value.strip_suffix("dB").ok_or(USAGE)?;
                gains[index] = Some(crate::gain::parse_eq_db(db)?);
            }
            "low-cut" | "high-cut" => {
                let target = if key == "low-cut" {
                    &mut low
                } else {
                    &mut high
                };
                if target.is_some() {
                    return Err("Give each EQ cutoff once.".into());
                }
                if value != "off" && !value.ends_with("Hz") {
                    return Err(USAGE.into());
                }
                *target = Some(crate::gain::parse_eq_cut(value)?);
            }
            _ => return Err(USAGE.into()),
        }
    }
    let gains = [
        gains[0].ok_or(USAGE)?,
        gains[1].ok_or(USAGE)?,
        gains[2].ok_or(USAGE)?,
    ];
    EqualizerRecipe::new(gains, low.flatten(), high.flatten())
        .map(|r| Entry::Equalizer(Some(r)))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eq_command_requires_complete_bounded_unit_bearing_recipes() {
        assert_eq!(super::super::parse(":eq"), Ok(Entry::EqualizerEdit));
        assert_eq!(super::super::parse("eq off"), Ok(Entry::Equalizer(None)));
        let Ok(Entry::Equalizer(Some(recipe))) = super::super::parse(
            "eq bass=3.125dB middle=-24dB treble=24dB low-cut=80Hz high-cut=12000Hz",
        ) else {
            panic!("valid EQ")
        };
        assert_eq!(
            recipe.gains().map(|gain| gain.millidecibels()),
            [3125, -24000, 24000]
        );
        assert_eq!(recipe.low_cut().unwrap().hz(), 80);
        assert_eq!(recipe.high_cut().unwrap().hz(), 12000);
        assert!(
            matches!(super::super::parse("eq bass=0dB middle=0dB treble=0dB low-cut=off"),Ok(Entry::Equalizer(Some(recipe))) if recipe.is_unity())
        );
        for input in [
            "eq off extra",
            "eq bass=1dB",
            "eq bass=0 middle=0dB treble=0dB",
            "eq bass=-24.001dB middle=0dB treble=0dB",
            "eq bass=NaNdB middle=0dB treble=0dB",
            "eq bass=0dB middle=0dB treble=0dB bass=1dB",
            "eq bass=0dB middle=0dB treble=0dB low-cut=80",
            "eq bass=0dB middle=0dB treble=0dB low-cut=79Hz",
            "eq bass=0dB middle=0dB treble=0dB high-cut=20001Hz",
            "eq bass=0dB middle=0dB treble=0dB low-cut=2000Hz high-cut=80Hz",
            "eq bass=0dB middle=0dB treble=0dB low-cut=off low-cut=80Hz",
        ] {
            assert!(super::super::parse(input).is_err(), "{input}");
        }
    }
}
