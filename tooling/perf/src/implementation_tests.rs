use super::*;
use std::num::NonZero;

#[test]
fn parses_total_and_segments() {
    let sample = parse_sample_output(
        "MD_PERF_SELF_TIMED_NS 30\nMD_PERF_SEGMENT_NS fixture_prepare 5\nMD_PERF_SEGMENT_NS source_first_draw 25\n",
    )
    .unwrap();
    assert_eq!(sample.total.as_nanos(), 30);
    assert_eq!(sample.segments.len(), 2);
    assert_eq!(sample.segments[0].name, "fixture_prepare");
    assert_eq!(sample.segments[1].name, "source_first_draw");
}

#[test]
fn rejects_total_only_output() {
    assert!(parse_sample_output("MD_PERF_SELF_TIMED_NS 30\n").is_err());
}

#[test]
fn rejects_invalid_segment_name() {
    assert!(
        parse_sample_output("MD_PERF_SELF_TIMED_NS 30\nMD_PERF_SEGMENT_NS bad-Name 10\n").is_err()
    );
}

#[test]
fn preserves_repeated_segments() {
    let sample = parse_sample_output(
        "MD_PERF_SELF_TIMED_NS 30\nMD_PERF_SEGMENT_NS redraw 5\nMD_PERF_SEGMENT_NS redraw 7\n",
    )
    .unwrap();
    assert_eq!(sample.segments.len(), 2);
    assert_eq!(sample.segments[0].name, "redraw");
    assert_eq!(sample.segments[1].name, "redraw");
}

#[test]
fn rejects_inconsistent_timelines() {
    let first = Sample {
        total: std::time::Duration::from_nanos(30),
        segments: vec![SegmentSample {
            name: "a".to_string(),
            duration: std::time::Duration::from_nanos(5),
        }],
    };
    let second = Sample {
        total: std::time::Duration::from_nanos(35),
        segments: vec![SegmentSample {
            name: "b".to_string(),
            duration: std::time::Duration::from_nanos(10),
        }],
    };
    assert!(Timings::from_samples(vec![first, second]).is_err());
}

#[test]
fn renders_segment_timeline_table() {
    let mut output = Output::blank();
    output.success(
        "case",
        TestMdata {
            version: consts::MDATA_VER,
            iterations: None,
            importance: Importance::Important,
            weight: 50,
        },
        NonZero::new(1).unwrap(),
        Timings::from_samples(vec![Sample {
            total: std::time::Duration::from_nanos(30),
            segments: vec![SegmentSample {
                name: "draw".to_string(),
                duration: std::time::Duration::from_nanos(10),
            }],
        }])
        .unwrap(),
    );

    let report = output.to_string();
    assert!(report.contains("| Command | # | Segment | Mean [ms] | SD [ms] | % total |"));
    assert!(report.contains("| case | 0 | draw |"));
}

#[test]
fn compares_matching_segment_occurrences() {
    let output = |duration| {
        let mut output = Output::blank();
        output.success(
            "case",
            TestMdata {
                version: consts::MDATA_VER,
                iterations: None,
                importance: Importance::Important,
                weight: 50,
            },
            NonZero::new(1).unwrap(),
            Timings::from_samples(vec![Sample {
                total: std::time::Duration::from_nanos(duration),
                segments: vec![SegmentSample {
                    name: "draw".to_string(),
                    duration: std::time::Duration::from_nanos(duration),
                }],
            }])
            .unwrap(),
        );
        output
    };

    let report = output(10).compare_perf(output(20)).to_string();
    assert!(report.contains("| Command | # | Segment | Delta |"));
    assert!(report.contains("| case | 0 | draw |"));
}
