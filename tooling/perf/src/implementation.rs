//! The implementation of the this crate is kept in a separate module
//! so that it is easy to publish this crate as part of GPUI's dependencies

use collections::HashMap;
use serde::{Deserialize, Serialize};
use std::{num::NonZero, time::Duration};

pub mod consts {
    //! Preset identifiers and constants so that the profiler and proc macro agree
    //! on their communication protocol.

    /// The suffix on the actual test function.
    pub const SUF_NORMAL: &str = "__MD_PERF_FN";
    /// The suffix on an extra function which prints metadata about a test to stdout.
    pub const SUF_MDATA: &str = "__MD_PERF_MDATA";
    /// The env var in which we pass the iteration count to our tests.
    pub const ITER_ENV_VAR: &str = "MD_PERF_ITER";
    /// The prefix printed on all benchmark test metadata lines, to distinguish it from
    /// possible output by the test harness itself.
    pub const MDATA_LINE_PREF: &str = "MD_MDATA_";
    /// The prefix printed by self-timed benchmarks before their measured duration in
    /// nanoseconds.
    pub const SELF_TIMED_LINE_PREF: &str = "MD_PERF_SELF_TIMED_NS";
    /// The prefix printed by self-timed benchmarks before a measured segment in
    /// nanoseconds.
    pub const SEGMENT_LINE_PREF: &str = "MD_PERF_SEGMENT_NS";
    /// The version number for the data returned from the test metadata function.
    /// Increment on non-backwards-compatible changes.
    pub const MDATA_VER: u32 = 0;
    /// The default weight, if none is specified.
    pub const WEIGHT_DEFAULT: u8 = 50;
    /// How long a test must have run to be assumed to be reliable-ish.
    pub const NOISE_CUTOFF: std::time::Duration = std::time::Duration::from_millis(250);

    /// Identifier for the iteration count of a test metadata.
    pub const ITER_COUNT_LINE_NAME: &str = "iter_count";
    /// Identifier for the weight of a test metadata.
    pub const WEIGHT_LINE_NAME: &str = "weight";
    /// Identifier for importance in test metadata.
    pub const IMPORTANCE_LINE_NAME: &str = "importance";
    /// Identifier for the test metadata version.
    pub const VERSION_LINE_NAME: &str = "version";

    /// Where to save json run information.
    pub const RUNS_DIR: &str = ".perf-runs";
}

/// How relevant a benchmark is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Importance {
    /// Regressions shouldn't be accepted without good reason.
    Critical = 4,
    /// Regressions should be paid extra attention.
    Important = 3,
    /// No extra attention should be paid to regressions, but they might still
    /// be indicative of something happening.
    #[default]
    Average = 2,
    /// Unclear if regressions are likely to be meaningful, but still worth keeping
    /// an eye on. Lowest level that's checked by default by the profiler.
    Iffy = 1,
    /// Regressions are likely to be spurious or don't affect core functionality.
    /// Only relevant if a lot of them happen, or as supplemental evidence for a
    /// higher-importance benchmark regressing. Not checked by default.
    Fluff = 0,
}

impl std::fmt::Display for Importance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Importance::Critical => f.write_str("critical"),
            Importance::Important => f.write_str("important"),
            Importance::Average => f.write_str("average"),
            Importance::Iffy => f.write_str("iffy"),
            Importance::Fluff => f.write_str("fluff"),
        }
    }
}

/// Why or when did this test fail?
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FailKind {
    /// Failed while triaging it to determine the iteration count.
    Triage,
    /// Failed while profiling it.
    Profile,
    /// Failed due to an incompatible version for the test.
    VersionMismatch,
    /// Could not parse metadata for a test.
    BadMetadata,
    /// Skipped due to filters applied on the perf run.
    Skipped,
}

impl std::fmt::Display for FailKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailKind::Triage => f.write_str("errored in triage"),
            FailKind::Profile => f.write_str("errored while profiling"),
            FailKind::VersionMismatch => f.write_str("test version mismatch"),
            FailKind::BadMetadata => f.write_str("bad test metadata"),
            FailKind::Skipped => f.write_str("skipped"),
        }
    }
}

/// Information about a given perf test.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestMdata {
    /// A version number for when the test was generated. If this is greater
    /// than the version this test handler expects, one of the following will
    /// happen in an unspecified manner:
    /// - The test is skipped silently.
    /// - The handler exits with an error message indicating the version mismatch
    ///   or inability to parse the metadata.
    ///
    /// INVARIANT: If `version` <= `MDATA_VER`, this tool *must* be able to
    /// correctly parse the output of this test.
    pub version: u32,
    /// How many iterations to pass this test if this is preset, or how many
    /// iterations a test ended up running afterwards if determined at runtime.
    pub iterations: Option<NonZero<usize>>,
    /// The importance of this particular test. See the docs on `Importance` for
    /// details.
    pub importance: Importance,
    /// The weight of this particular test within its importance category. Used
    /// when comparing across runs.
    pub weight: u8,
}

/// One named segment reported by a benchmark sample.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentSample {
    /// Stable lowercase segment identifier.
    pub name: String,
    /// Time spent in this segment.
    pub duration: Duration,
}

/// One self-timed sample reported by a benchmark.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sample {
    /// Total measured time for the sample.
    pub total: Duration,
    /// Ordered segment timeline for the sample.
    pub segments: Vec<SegmentSample>,
}

/// Aggregate statistics for a duration series.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SampleSummary {
    /// Mean runtime.
    pub mean: Duration,
    /// Standard deviation for the runtime.
    pub stddev: Duration,
}

/// Aggregate statistics for one segment occurrence in the timeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SegmentSummary {
    /// Zero-based occurrence index in the timeline.
    pub index: usize,
    /// Stable lowercase segment identifier.
    pub name: String,
    /// Mean and standard deviation for this occurrence.
    pub timings: SampleSummary,
}

/// The actual timings of a test's self-reported measured region.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Timings {
    /// Mean runtime for `self.iter_total` runs of this test.
    pub mean: Duration,
    /// Standard deviation for the above.
    pub stddev: Duration,
    /// Raw self-timed samples collected for this test.
    pub samples: Vec<Sample>,
    /// Aggregated segment timeline, grouped by occurrence.
    pub segments: Vec<SegmentSummary>,
}

impl Timings {
    /// How many iterations does this test seem to do per second?
    #[expect(
        clippy::cast_precision_loss,
        reason = "We only care about a couple sig figs anyways"
    )]
    #[must_use]
    pub fn iters_per_sec(&self, total_iters: NonZero<usize>) -> f64 {
        (1000. / self.mean.as_millis() as f64) * total_iters.get() as f64
    }

    /// Builds aggregate timings from raw samples.
    ///
    /// # Errors
    /// Returns `FailKind::Profile` when samples are empty or segment timelines differ.
    pub fn from_samples(samples: Vec<Sample>) -> Result<Self, FailKind> {
        if samples.is_empty() || samples[0].segments.is_empty() {
            return Err(FailKind::Profile);
        }

        let first_timeline = samples[0]
            .segments
            .iter()
            .map(|segment| segment.name.as_str())
            .collect::<Vec<_>>();
        for sample in &samples[1..] {
            let timeline = sample
                .segments
                .iter()
                .map(|segment| segment.name.as_str())
                .collect::<Vec<_>>();
            if timeline != first_timeline {
                return Err(FailKind::Profile);
            }
        }

        let total = summarize(samples.iter().map(|sample| sample.total));
        let segments = first_timeline
            .iter()
            .enumerate()
            .map(|(index, name)| SegmentSummary {
                index,
                name: (*name).to_string(),
                timings: summarize(samples.iter().map(|sample| sample.segments[index].duration)),
            })
            .collect();

        Ok(Self {
            mean: total.mean,
            stddev: total.stddev,
            samples,
            segments,
        })
    }
}

/// Parses one benchmark sample from test process output.
///
/// # Errors
/// Returns `FailKind::Profile` for missing totals, repeated totals, illegal segment
/// names, or invalid durations.
pub fn parse_sample_output(output: &str) -> Result<Sample, FailKind> {
    let mut total = None;
    let mut segments = Vec::new();

    for line in output.lines() {
        if let Some(value) = line.strip_prefix(consts::SELF_TIMED_LINE_PREF) {
            if total.is_some() {
                return Err(FailKind::Profile);
            }
            let value = value.trim().parse::<u64>().map_err(|_| FailKind::Profile)?;
            total = Some(Duration::from_nanos(value));
        } else if let Some(rest) = line.strip_prefix(consts::SEGMENT_LINE_PREF) {
            let mut fields = rest.split_whitespace();
            let name = fields.next().ok_or(FailKind::Profile)?;
            let duration = fields
                .next()
                .ok_or(FailKind::Profile)?
                .parse::<u64>()
                .map_err(|_| FailKind::Profile)?;
            if fields.next().is_some() || !is_valid_segment_name(name) {
                return Err(FailKind::Profile);
            }
            segments.push(SegmentSample {
                name: name.to_string(),
                duration: Duration::from_nanos(duration),
            });
        }
    }

    if segments.is_empty() {
        return Err(FailKind::Profile);
    }

    Ok(Sample {
        total: total.ok_or(FailKind::Profile)?,
        segments,
    })
}

/// Validates a segment identifier.
fn is_valid_segment_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Summarizes a duration series.
fn summarize(samples: impl IntoIterator<Item = Duration>) -> SampleSummary {
    let samples = samples.into_iter().collect::<Vec<_>>();
    let sample_count = samples.len() as f64;
    let mean_secs = samples.iter().map(Duration::as_secs_f64).sum::<f64>() / sample_count;
    let stddev_secs = (samples
        .iter()
        .map(|sample| {
            let delta = sample.as_secs_f64() - mean_secs;
            delta * delta
        })
        .sum::<f64>()
        / sample_count)
        .sqrt();

    SampleSummary {
        mean: Duration::from_secs_f64(mean_secs),
        stddev: Duration::from_secs_f64(stddev_secs),
    }
}

/// Aggregate results, meant to be used for a given importance category. Each
/// test name corresponds to its benchmark results, iteration count, and weight.
type CategoryInfo = HashMap<String, (Timings, NonZero<usize>, u8)>;

/// Comparison row for one matching segment occurrence.
struct SegmentDelta {
    /// Test case name.
    case: String,
    /// Segment occurrence index.
    index: usize,
    /// Segment name.
    name: String,
    /// Relative timing shift.
    shift: f64,
}

/// Aggregate output of all tests run by this handler.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Output {
    /// A list of test outputs. Format is `(test_name, mdata, timings)`.
    /// The latter being `Ok(_)` indicates the test succeeded.
    ///
    /// INVARIANT: If the test succeeded, the second field is `Some(mdata)` and
    /// `mdata.iterations` is `Some(_)`.
    tests: Vec<(String, Option<TestMdata>, Result<Timings, FailKind>)>,
}

impl Output {
    /// Instantiates an empty "output". Useful for merging.
    #[must_use]
    pub fn blank() -> Self {
        Output { tests: Vec::new() }
    }

    /// Reports a success and adds it to this run's `Output`.
    pub fn success(
        &mut self,
        name: impl AsRef<str>,
        mut mdata: TestMdata,
        iters: NonZero<usize>,
        timings: Timings,
    ) {
        mdata.iterations = Some(iters);
        self.tests
            .push((name.as_ref().to_string(), Some(mdata), Ok(timings)));
    }

    /// Reports a failure and adds it to this run's `Output`. If this test was tried
    /// with some number of iterations (i.e. this was not a version mismatch or skipped
    /// test), it should be reported also.
    ///
    /// Using the `fail!()` macro is usually more convenient.
    pub fn failure(
        &mut self,
        name: impl AsRef<str>,
        mut mdata: Option<TestMdata>,
        attempted_iters: Option<NonZero<usize>>,
        kind: FailKind,
    ) {
        if let Some(ref mut mdata) = mdata {
            mdata.iterations = attempted_iters;
        }
        self.tests
            .push((name.as_ref().to_string(), mdata, Err(kind)));
    }

    /// True if no tests executed this run.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tests.is_empty()
    }

    /// Sorts the runs in the output in the order that we want them printed.
    pub fn sort(&mut self) {
        self.tests.sort_unstable_by(|a, b| match (a, b) {
            // Tests where we got no metadata go at the end.
            ((_, Some(_), _), (_, None, _)) => std::cmp::Ordering::Greater,
            ((_, None, _), (_, Some(_), _)) => std::cmp::Ordering::Less,
            // Then sort by importance, then weight.
            ((_, Some(a_mdata), _), (_, Some(b_mdata), _)) => {
                let c = a_mdata.importance.cmp(&b_mdata.importance);
                if matches!(c, std::cmp::Ordering::Equal) {
                    a_mdata.weight.cmp(&b_mdata.weight)
                } else {
                    c
                }
            }
            // Lastly by name.
            ((a_name, ..), (b_name, ..)) => a_name.cmp(b_name),
        });
    }

    /// Merges the output of two runs, appending a prefix to the results of the new run.
    /// To be used in conjunction with `Output::blank()`, or else only some tests will have
    /// a prefix set.
    pub fn merge<'a>(&mut self, other: Self, pref_other: impl Into<Option<&'a str>>) {
        let pref = if let Some(pref) = pref_other.into() {
            "crates/".to_string() + pref + "::"
        } else {
            String::new()
        };
        self.tests = std::mem::take(&mut self.tests)
            .into_iter()
            .chain(
                other
                    .tests
                    .into_iter()
                    .map(|(name, md, tm)| (pref.clone() + &name, md, tm)),
            )
            .collect();
    }

    /// Evaluates the performance of `self` against `baseline`. The latter is taken
    /// as the comparison point, i.e. a positive resulting `PerfReport` means that
    /// `self` performed better.
    ///
    /// # Panics
    /// `self` and `baseline` are assumed to have the iterations field on all
    /// `TestMdata`s set to `Some(_)` if the `TestMdata` is present itself.
    #[must_use]
    pub fn compare_perf(self, baseline: Self) -> PerfReport {
        let segment_deltas = self.segment_deltas(&baseline);
        let self_categories = self.collapse();
        let mut other_categories = baseline.collapse();

        let deltas = self_categories
            .into_iter()
            .filter_map(|(cat, self_data)| {
                // Only compare categories where both runs have data.
                let mut other_data = other_categories.remove(&cat)?;
                let mut max = f64::MIN;
                let mut min = f64::MAX;

                // Running totals for averaging out tests.
                let mut r_total_numerator = 0.;
                let mut r_total_denominator = 0;
                // Yeah this is O(n^2), but realistically it'll hardly be a bottleneck.
                for (name, (s_timings, s_iters, weight)) in self_data {
                    // Only use the new weights if they conflict.
                    let Some((o_timings, o_iters, _)) = other_data.remove(&name) else {
                        continue;
                    };
                    let shift =
                        (s_timings.iters_per_sec(s_iters) / o_timings.iters_per_sec(o_iters)) - 1.;
                    if shift > max {
                        max = shift;
                    }
                    if shift < min {
                        min = shift;
                    }
                    r_total_numerator += shift * f64::from(weight);
                    r_total_denominator += u32::from(weight);
                }
                // There were no runs here!
                if r_total_denominator == 0 {
                    None
                } else {
                    let mean = r_total_numerator / f64::from(r_total_denominator);
                    // TODO: also aggregate standard deviation? That's harder to keep
                    // meaningful, though, since we dk which tests are correlated.
                    Some((cat, PerfDelta { max, mean, min }))
                }
            })
            .collect();

        PerfReport {
            deltas,
            segment_deltas,
        }
    }

    /// Computes deltas for matching segment occurrences.
    fn segment_deltas(&self, baseline: &Self) -> Vec<SegmentDelta> {
        let mut baseline_cases = HashMap::<&str, &Timings>::default();
        for (name, _, timings) in &baseline.tests {
            if let Ok(timings) = timings {
                baseline_cases.insert(name, timings);
            }
        }

        let mut deltas = Vec::new();
        for (case, _, timings) in &self.tests {
            let Ok(timings) = timings else {
                continue;
            };
            let Some(baseline_timings) = baseline_cases.get(case.as_str()) else {
                continue;
            };
            for (segment, baseline_segment) in
                timings.segments.iter().zip(&baseline_timings.segments)
            {
                if segment.index != baseline_segment.index || segment.name != baseline_segment.name
                {
                    continue;
                }
                let shift = (baseline_segment.timings.mean.as_secs_f64()
                    / segment.timings.mean.as_secs_f64())
                    - 1.;
                deltas.push(SegmentDelta {
                    case: case.clone(),
                    index: segment.index,
                    name: segment.name.clone(),
                    shift,
                });
            }
        }

        deltas
    }

    /// Collapses the `PerfReport` into a `HashMap` over `Importance`, with
    /// each importance category having its tests contained.
    fn collapse(self) -> HashMap<Importance, CategoryInfo> {
        let mut categories = HashMap::<Importance, HashMap<String, _>>::default();
        for entry in self.tests {
            if let Some(mdata) = entry.1
                && let Ok(timings) = entry.2
            {
                if let Some(handle) = categories.get_mut(&mdata.importance) {
                    handle.insert(entry.0, (timings, mdata.iterations.unwrap(), mdata.weight));
                } else {
                    let mut new = HashMap::default();
                    new.insert(entry.0, (timings, mdata.iterations.unwrap(), mdata.weight));
                    categories.insert(mdata.importance, new);
                }
            }
        }

        categories
    }
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't print the header for an empty run.
        if self.tests.is_empty() {
            return Ok(());
        }

        // We want to print important tests at the top, then alphabetical.
        let mut sorted = self.clone();
        sorted.sort();
        // Markdown header for making a nice little table :>
        writeln!(
            f,
            "| Command | Iter/sec | Mean [ms] | SD [ms] | Iterations | Importance (weight) |",
        )?;
        writeln!(f, "|:---|---:|---:|---:|---:|---:|")?;
        for (name, metadata, timings) in &sorted.tests {
            match metadata {
                Some(metadata) => match timings {
                    // Happy path.
                    Ok(timings) => {
                        // If the test succeeded, then metadata.iterations is Some(_).
                        writeln!(
                            f,
                            "| {} | {:.2} | {} | {:.2} | {} | {} ({}) |",
                            name,
                            timings.iters_per_sec(metadata.iterations.unwrap()),
                            {
                                // Very small mean runtimes will give inaccurate
                                // results. Should probably also penalise weight.
                                let mean = timings.mean.as_secs_f64() * 1000.;
                                if mean < consts::NOISE_CUTOFF.as_secs_f64() * 1000. / 8. {
                                    format!("{mean:.2} (unreliable)")
                                } else {
                                    format!("{mean:.2}")
                                }
                            },
                            timings.stddev.as_secs_f64() * 1000.,
                            metadata.iterations.unwrap(),
                            metadata.importance,
                            metadata.weight,
                        )?;
                    }
                    // We have (some) metadata, but the test errored.
                    Err(err) => writeln!(
                        f,
                        "| ({}) {} | N/A | N/A | N/A | {} | {} ({}) |",
                        err,
                        name,
                        metadata
                            .iterations
                            .map_or_else(|| "N/A".to_owned(), |i| format!("{i}")),
                        metadata.importance,
                        metadata.weight
                    )?,
                },
                // No metadata, couldn't even parse the test output.
                None => writeln!(
                    f,
                    "| ({}) {} | N/A | N/A | N/A | N/A | N/A |",
                    timings.as_ref().unwrap_err(),
                    name
                )?,
            }
        }
        let segment_rows = sorted
            .tests
            .iter()
            .filter_map(|(name, _, timings)| timings.as_ref().ok().map(|timings| (name, timings)))
            .flat_map(|(name, timings)| {
                timings.segments.iter().map(move |segment| {
                    let mean_ms = segment.timings.mean.as_secs_f64() * 1000.;
                    let stddev_ms = segment.timings.stddev.as_secs_f64() * 1000.;
                    let pct_total =
                        segment.timings.mean.as_secs_f64() / timings.mean.as_secs_f64() * 100.;
                    (name, segment, mean_ms, stddev_ms, pct_total)
                })
            })
            .collect::<Vec<_>>();

        if !segment_rows.is_empty() {
            writeln!(f)?;
            writeln!(
                f,
                "| Command | # | Segment | Mean [ms] | SD [ms] | % total |"
            )?;
            writeln!(f, "|:---|---:|:---|---:|---:|---:|")?;
            for (name, segment, mean_ms, stddev_ms, pct_total) in segment_rows {
                writeln!(
                    f,
                    "| {} | {} | {} | {:.2} | {:.2} | {:.1}% |",
                    name, segment.index, segment.name, mean_ms, stddev_ms, pct_total
                )?;
            }
        }
        Ok(())
    }
}

/// The difference in performance between two runs within a given importance
/// category.
struct PerfDelta {
    /// The biggest improvement / least bad regression.
    max: f64,
    /// The weighted average change in test times.
    mean: f64,
    /// The worst regression / smallest improvement.
    min: f64,
}

/// Shim type for reporting all performance deltas across importance categories.
pub struct PerfReport {
    /// Inner (group, diff) pairing.
    deltas: HashMap<Importance, PerfDelta>,
    /// Matching segment occurrence deltas.
    segment_deltas: Vec<SegmentDelta>,
}

impl std::fmt::Display for PerfReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.deltas.is_empty() && self.segment_deltas.is_empty() {
            return write!(f, "(no matching tests)");
        }
        if !self.deltas.is_empty() {
            let sorted = self.deltas.iter().collect::<Vec<_>>();
            writeln!(f, "| Category | Max | Mean | Min |")?;
            write!(f, "|:---|---:|---:|---:|")?;
            for (cat, delta) in sorted.into_iter().rev() {
                write!(
                    f,
                    "\n| {cat} | {} | {} | {} |",
                    prettify_delta(delta.max),
                    prettify_delta(delta.mean),
                    prettify_delta(delta.min)
                )?;
            }
        }
        if !self.segment_deltas.is_empty() {
            if !self.deltas.is_empty() {
                writeln!(f)?;
            }
            writeln!(f, "| Command | # | Segment | Delta |")?;
            write!(f, "|:---|---:|:---|---:|")?;
            for delta in &self.segment_deltas {
                write!(
                    f,
                    "\n| {} | {} | {} | {} |",
                    delta.case,
                    delta.index,
                    delta.name,
                    prettify_delta(delta.shift)
                )?;
            }
        }
        Ok(())
    }
}

/// Pretty-prints a relative performance delta.
fn prettify_delta(time: f64) -> String {
    const SIGN_POS: &str = "up";
    const SIGN_NEG: &str = "down";
    const SIGN_NEUTRAL_POS: &str = "near up";
    const SIGN_NEUTRAL_NEG: &str = "near down";

    let sign = if time > 0.05 {
        SIGN_POS
    } else if time > 0. {
        SIGN_NEUTRAL_POS
    } else if time > -0.05 {
        SIGN_NEUTRAL_NEG
    } else {
        SIGN_NEG
    };
    format!("{} {:.1}%", sign, time.abs() * 100.)
}
