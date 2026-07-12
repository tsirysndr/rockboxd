// End-to-end smoke test for the Go bindings.
//
// Run from the binding root: go run ./examples/smoke
package main

import (
	"fmt"
	"log"
	"path/filepath"
	"runtime"
	"time"

	rockbox "github.com/tsirysndr/rockboxd/bindings/go"
)

func main() {
	// Locate the repo root relative to this source file.
	_, file, _, _ := runtime.Caller(0)
	repo := filepath.Join(filepath.Dir(file), "..", "..", "..", "..")
	fixture := filepath.Join(repo, "crates", "rocksky", "fixtures",
		"08 - Internet Money - Speak(Explicit).m4a")

	fmt.Printf("ABI version: %d\n", rockbox.ABIVersion())

	// -- metadata ---------------------------------------------------------
	meta, err := rockbox.Metadata.Read(fixture)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("codec=%q title=%q artist=%q duration_ms=%d sample_rate=%d\n",
		meta.Codec, meta.Title, meta.Artist, meta.DurationMs, meta.SampleRate)
	if meta.Codec == "" {
		log.Fatal("codec label should not be empty")
	}

	probe, ok := rockbox.Metadata.Probe("song.flac")
	fmt.Printf("Probe(\"song.flac\") = %q (ok=%v)\n", probe, ok)
	if probe != "FLAC" {
		log.Fatalf("expected FLAC, got %q", probe)
	}

	// -- DSP: -6.0206 dB track gain should HALVE amplitude ----------------
	const rate = 44100
	dsp, err := rockbox.NewDsp(rate)
	if err != nil {
		log.Fatal(err)
	}
	defer dsp.Close()

	dsp.SetReplaygain(rockbox.DspReplayGainTrack, false, 0.0)
	dsp.SetReplaygainGains(rockbox.Opt(-6.0206), nil, nil, nil) // x0.5

	sine := rockbox.SineStereo(1000, 1.0, rate, 16000)
	out, err := dsp.Process(sine)
	if err != nil {
		log.Fatal(err)
	}
	peak := 0
	for _, s := range out {
		v := int(s)
		if v < 0 {
			v = -v
		}
		if v > peak {
			peak = v
		}
	}
	fmt.Printf("DSP peak after -6.02 dB track gain: %d (expected ~8000)\n", peak)
	if peak < 7600 || peak > 8400 {
		log.Fatalf("peak %d out of range", peak)
	}

	// -- Player (construct only, no audible playback) ---------------------
	cfg := rockbox.DefaultConfig()
	cfg.Volume = 0.0
	player, err := rockbox.NewPlayer(cfg)
	if err != nil {
		log.Fatal(err)
	}
	defer player.Close()

	sr := player.SampleRate()
	fmt.Printf("Player sample_rate=%d\n", sr)
	if sr == 0 {
		log.Fatal("sample_rate should be > 0")
	}
	player.SetVolume(0.0)
	if err := player.SetQueue([]string{fixture}); err != nil {
		log.Fatal(err)
	}
	// DSP: Bass Boost preset + a +7 dB bass / +4 dB treble lift.
	player.SetEqPreset(rockbox.EqPresetBassBoost)
	player.SetBass(7)
	player.SetTreble(4)
	time.Sleep(100 * time.Millisecond) // queue command is applied asynchronously

	st, err := player.Status()
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Player status: state=%q queue_len=%d\n", st.State, st.QueueLen)
	if st.State != "stopped" {
		log.Fatalf("expected stopped, got %q", st.State)
	}
	if st.QueueLen != 1 {
		log.Fatalf("expected queue_len 1, got %d", st.QueueLen)
	}

	fmt.Println("\nALL CHECKS PASSED ✔")
}
