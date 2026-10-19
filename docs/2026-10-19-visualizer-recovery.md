# Visualizer recovery, October 19, 2026

The installed 0.18.3 GUI was still reading a `tronteq.controls-only` marker
left by the October 16 lag investigation. This deliberately skipped shader
initialization and WASAPI capture on every launch. The VIZ panel then reported
the missing renderer as unavailable, even though no shader initialization had
been attempted.

The marker and installed executable were backed up under
`target/repair-20261019/`, then the marker was removed. The restarted GUI
compiled its shaders, opened Beyond TV at 48 kHz stereo float, and settled at
60 FPS. Its own framebuffer capture is `target/repair-20261019/viz-restored.png`.
The visible process used 1.0 CPU second during a 10.04-second sample and
remained responsive. This confirms the visualizer was restored; it does not
establish that the historical desktop lag can never recur.

Normal launches now ignore the legacy marker. `--controls-only` remains an
explicit per-launch diagnostic option. Existing uncommitted 0.18.4 work from
the earlier investigation was preserved.

The CLI now includes read-only `audio-status`: endpoint volume, mute, peak,
and session PID/state/volume/mute/peak. Beyond TV was the default at 73%,
unmuted, with Chrome unmuted at 100% and live audio. APO telemetry advanced.
A temporary bypass produced matching input/output levels, but Trent still
heard silence and chose to check the TV settings. Audio was subsequently
observed with bypass off again; the test detected an intervening state change
and did not overwrite it. No audio service or APO installation was changed
by this repair.

Validation: 141 GUI tests, 4 CLI tests, and 13 shared tests passed (158 total).
The optional live capture test was excluded from that suite; the actual GUI
capture was verified above. Logs: `target/repair-tests.log` and
`target/repair-build.log`.

The release build succeeded and 0.18.4 was installed at the normal per-user
path. The installed GUI hash matches the built artifact; the deployment receipt
is `target/repair-20261019/installed-receipt.json`. It compiled shaders,
captured Beyond TV audio, settled at 60 FPS, and produced the verified frame
`target/repair-20261019/viz-0.18.4.png`. The legacy marker is absent. Trent
confirmed "ITS WORKING NOW BTW" after deployment.
