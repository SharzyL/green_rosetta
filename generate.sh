#!/usr/bin/env bash

ffmpeg -i ./test.flac \
  -map 0:a -c:a:0 aac -b:a:0 128k \
  -map 0:a -c:a:1 libopus -b:a:1 160k \
  -hls_segment_type fmp4 \
  -hls_time 6 \
  -hls_list_size 0 \
  -hls_playlist_type vod \
  -var_stream_map "a:0,agroup:audio,name:aac_128k a:1,agroup:audio,name:opus_128k" \
  -hls_fmp4_init_filename "init_%v.mp4" \
  -hls_segment_filename "test_dir/chunk_%v_%03d.m4s" \
  -master_pl_name "master.m3u8" \
  "test_dir/stream_%v.m3u8"

