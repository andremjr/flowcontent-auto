import unittest

from segmenter import (
    DEFAULT_PAUSE_THRESHOLD_MS,
    MIN_ASSET_DURATION_MS,
    Word,
    render_asset_srt,
    render_srt,
    segment_assets,
    segment_captions,
)


class SegmenterTests(unittest.TestCase):
    def test_caption_segmentation_uses_configured_word_count(self):
        words = [
            Word("um", 0, 400),
            Word("dois", 500, 900),
            Word("três", 1_000, 1_400),
            Word("quatro", 1_500, 1_900),
            Word("cinco", 2_000, 2_400),
        ]
        segments = segment_captions(words, 2)
        self.assertEqual([segment.text for segment in segments], ["um dois", "três quatro", "cinco"])

    def test_asset_count_still_respects_the_eight_second_ceiling(self):
        words = [
            Word("Começo", 0, 1_000),
            Word("forte.", 1_100, 2_000),
            Word("Outra", 2_100, 3_000),
            Word("parte", 3_100, 4_000),
            Word("continua", 4_100, 7_900),
            Word("depois", 8_000, 8_700),
        ]
        segments = segment_assets(words, 8_000)
        self.assertEqual(len(segments), 2)
        self.assertLessEqual(max(segment.duration_ms for segment in segments), 8_000)
        self.assertEqual(" ".join(segment.text for segment in segments), " ".join(word.text for word in words))

    def test_internal_cut_prefers_real_pause_instead_of_comma(self):
        words = [
            Word("um", 0, 1_000),
            Word("dois,", 1_100, 2_000),
            Word("três", 2_100, 3_000),
            Word("quatro", 4_000, 7_900),
            Word("cinco", 8_100, 10_000),
        ]
        segments = segment_assets(words, 8_000, pause_threshold_ms=1_500)
        self.assertEqual(segments[0].text, "um dois, três")
        self.assertEqual(segments[0].pause_after_ms, 1_000)
        self.assertEqual(segments[0].part_count, 2)

    def test_default_pause_threshold_is_one_hundred_milliseconds(self):
        words = [
            Word("Primeira.", 0, 3_000),
            Word("Segunda.", 3_100, 6_200),
        ]
        segments = segment_assets(words)
        self.assertEqual(len(segments), 2)
        self.assertEqual(segments[0].end, 3_050)
        self.assertEqual(segments[1].start, 3_050)
        self.assertEqual(segments[0].pause_after_ms, DEFAULT_PAUSE_THRESHOLD_MS)

    def test_short_pause_groups_are_merged_until_they_reach_three_seconds(self):
        words = [
            Word("um", 0, 900),
            Word("dois", 1_200, 2_100),
            Word("tres", 4_700, 7_900),
        ]
        segments = segment_assets(words, timeline_start_ms=0, timeline_end_ms=8_200)
        self.assertEqual(len(segments), 2)
        self.assertEqual(segments[0].text, "um dois")
        self.assertGreaterEqual(segments[0].duration_ms, MIN_ASSET_DURATION_MS)
        self.assertGreaterEqual(segments[1].duration_ms, MIN_ASSET_DURATION_MS)

    def test_partitioning_prefers_parts_with_at_least_three_seconds_when_possible(self):
        words = [
            Word("um", 0, 1_500),
            Word("dois", 1_600, 3_200),
            Word("tres", 3_300, 6_500),
            Word("quatro", 6_600, 9_200),
        ]
        segments = segment_assets(words, pause_threshold_ms=2_000)
        self.assertEqual(len(segments), 2)
        self.assertTrue(all(segment.duration_ms >= MIN_ASSET_DURATION_MS for segment in segments))

    def test_no_word_is_lost_or_reordered(self):
        words = [Word(f"w{index}", index * 1_000, index * 1_000 + 700) for index in range(20)]
        segments = segment_assets(words, 8_000)
        rebuilt = " ".join(segment.text for segment in segments)
        self.assertEqual(rebuilt, " ".join(word.text for word in words))
        self.assertTrue(all(segment.duration_ms <= 8_000 for segment in segments))

    def test_srt_uses_stable_asset_order(self):
        segments = segment_assets([Word("Olá.", 0, 1_000), Word("Fim.", 1_100, 2_000)])
        self.assertIn("00:00:00,000 --> 00:00:02,000", render_srt(segments))
        self.assertEqual([segment.segment_id for segment in segments], ["asset-0001"])

    def test_long_pause_unit_repeats_full_context_for_external_prompt_generation(self):
        words = [
            Word("Aquele", 0, 1_000),
            Word("mundo", 1_100, 2_000),
            Word("mágico", 2_100, 3_000),
            Word("não", 3_100, 4_000),
            Word("era,", 4_100, 5_000),
            Word("nem", 5_400, 6_200),
            Word("de", 6_300, 7_100),
            Word("longe,", 7_200, 8_100),
            Word("parecido", 8_500, 9_500),
            Word("com", 9_600, 10_300),
            Word("o", 10_400, 10_900),
            Word("mundo", 11_000, 11_800),
            Word("real.", 11_900, 12_600),
        ]
        segments = segment_assets(words, 8_000, pause_threshold_ms=1_000)
        self.assertEqual(len(segments), 2)
        self.assertEqual(segments[0].context_text, " ".join(word.text for word in words))
        self.assertNotIn("CONTEXTO:", render_asset_srt(segments))
        self.assertIn("Aquele mundo mágico não era,", render_asset_srt(segments))

    def test_midpoint_mode_splits_silence_without_timeline_gap(self):
        words = [
            Word("Primeira.", 1_000, 3_000),
            Word("Segunda.", 5_000, 6_000),
        ]
        segments = segment_assets(
            words,
            pause_threshold_ms=600,
            transition_mode="midpoint",
            timeline_start_ms=0,
            timeline_end_ms=8_000,
        )
        self.assertEqual((segments[0].start, segments[0].end), (0, 4_000))
        self.assertEqual((segments[1].start, segments[1].end), (4_000, 8_000))

    def test_next_speech_mode_keeps_whole_pause_on_previous_asset(self):
        words = [
            Word("Primeira.", 1_000, 3_000),
            Word("Segunda.", 5_000, 6_000),
        ]
        segments = segment_assets(
            words,
            pause_threshold_ms=600,
            transition_mode="next-speech",
            timeline_start_ms=0,
            timeline_end_ms=8_000,
        )
        self.assertEqual(segments[0].end, 5_000)
        self.assertEqual(segments[1].start, 5_000)

    def test_previous_speech_mode_gives_whole_pause_to_next_asset(self):
        words = [
            Word("Primeira.", 1_000, 3_000),
            Word("Segunda.", 5_000, 6_000),
        ]
        segments = segment_assets(
            words,
            pause_threshold_ms=600,
            transition_mode="previous-speech",
            timeline_start_ms=0,
            timeline_end_ms=8_000,
        )
        self.assertEqual(segments[0].end, 3_000)
        self.assertEqual(segments[1].start, 3_000)


if __name__ == "__main__":
    unittest.main()
