// Play an audio source through the real output device.
//
// The queue entry can be a local file, a remote http(s):// file, an
// internet-radio stream, or an HLS (.m3u8) / MPEG-DASH (.mpd) manifest —
// the engine detects each kind automatically.
//
// Run from the binding root:
//
//	go run ./examples/play [path-or-URL]
//	go run ./examples/play hls    # public HLS test stream
//	go run ./examples/play dash   # public MPEG-DASH test stream
package main

import (
	"fmt"
	"log"
	"os"
	"os/signal"
	"path/filepath"
	"runtime"
	"time"

	rockbox "github.com/tsirysndr/rockboxd/bindings/go"
)

// Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
// for more).
const (
	hlsDefault  = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8"
	dashDefault = "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd"
)

func main() {
	// Locate the repo root relative to this source file.
	_, file, _, _ := runtime.Caller(0)
	repo := filepath.Join(filepath.Dir(file), "..", "..", "..", "..")
	fixture := filepath.Join(repo, "crates", "rocksky", "fixtures",
		"08 - Internet Money - Speak(Explicit).m4a")

	source := fixture
	if len(os.Args) > 1 {
		source = os.Args[1]
	}
	switch source {
	case "hls":
		source = hlsDefault
	case "dash":
		source = dashDefault
	}

	cfg := rockbox.DefaultConfig()
	cfg.Volume = 0.8
	player, err := rockbox.NewPlayer(cfg)
	if err != nil {
		log.Fatal(err)
	}
	defer player.Close()

	if err := player.SetQueue([]string{source}); err != nil {
		log.Fatal(err)
	}
	// DSP: Bass Boost preset + a +7 dB bass / +4 dB treble lift.
	player.SetEqPreset(rockbox.EqPresetBassBoost)
	player.SetBass(7)
	player.SetTreble(4)
	player.Play()
	fmt.Printf("▶ playing %s\n", source)
	fmt.Println("eq: BassBoost preset, bass +7 dB, treble +4 dB")

	// Install a SIGINT handler AFTER the player boots: the native audio
	// engine installs its own signal handler while starting the output
	// device, which otherwise swallows Ctrl-C. We os.Exit() straight away
	// instead of calling player.Stop()/Close() — those are blocking native
	// calls that can deadlock against the engine thread. The OS reclaims
	// the output device on exit.
	sigc := make(chan os.Signal, 1)
	signal.Notify(sigc, os.Interrupt)
	go func() {
		<-sigc
		fmt.Println("\nstopped")
		os.Exit(130)
	}()

	// Poll status until playback finishes (state returns to "stopped").
	// A live stream reports duration 0 and plays until Ctrl-C.
	for {
		st, err := player.Status()
		if err != nil {
			log.Fatal(err)
		}
		pos := float64(st.PositionMs) / 1000
		clock := fmt.Sprintf("%.1fs / %.1fs", pos, float64(st.DurationMs)/1000)
		if st.DurationMs == 0 {
			clock = fmt.Sprintf("%.1fs / LIVE", pos)
		}
		// The codec label carries the protocol for adaptive streams
		// (e.g. "HLS AAC").
		codec := ""
		if st.Metadata != nil {
			codec = st.Metadata.Codec
		}
		fmt.Printf("\r[%s] %s %s   ", st.State, codec, clock)
		if st.State == "stopped" && st.PositionMs > 0 {
			fmt.Println("\n✔ done")
			break
		}
		time.Sleep(500 * time.Millisecond)
	}
}
