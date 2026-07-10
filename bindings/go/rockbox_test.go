package rockbox_test

import (
	"path/filepath"
	"runtime"
	"time"

	"testing"

	rockbox "github.com/tsirysndr/rockboxd/bindings/go"
)

func fixture(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("cannot locate test source")
	}
	repo := filepath.Join(filepath.Dir(file), "..", "..")
	return filepath.Join(repo, "crates", "rocksky", "fixtures",
		"08 - Internet Money - Speak(Explicit).m4a")
}

func TestABIVersion(t *testing.T) {
	if got := rockbox.ABIVersion(); got != 1 {
		t.Fatalf("ABIVersion() = %d, want 1", got)
	}
}

func TestMetadataReadAndProbe(t *testing.T) {
	meta, err := rockbox.Metadata.Read(fixture(t))
	if err != nil {
		t.Fatal(err)
	}
	if meta.Codec != "AAC" {
		t.Errorf("Codec = %q, want AAC", meta.Codec)
	}
	if meta.SampleRate != 44100 {
		t.Errorf("SampleRate = %d, want 44100", meta.SampleRate)
	}
	if meta.DurationMs == 0 {
		t.Error("DurationMs should be > 0")
	}

	if got, ok := rockbox.Metadata.Probe("song.flac"); !ok || got != "FLAC" {
		t.Errorf("Probe(song.flac) = %q, %v; want FLAC, true", got, ok)
	}
	if _, ok := rockbox.Metadata.Probe("nope.xyz"); ok {
		t.Error("Probe(nope.xyz) should report ok=false")
	}
}

func TestDspReplaygainHalvesAmplitude(t *testing.T) {
	dsp, err := rockbox.NewDsp(44100)
	if err != nil {
		t.Fatal(err)
	}
	defer dsp.Close()

	dsp.SetReplaygain(rockbox.DspReplayGainTrack, false, 0.0)
	dsp.SetReplaygainGains(rockbox.Opt(-6.0206), nil, nil, nil) // x0.5

	out, err := dsp.Process(rockbox.SineStereo(1000, 1.0, 44100, 16000))
	if err != nil {
		t.Fatal(err)
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
	if peak < 7600 || peak > 8400 {
		t.Errorf("peak = %d, want ~8000", peak)
	}
}

func TestPlayerConstructAndStatus(t *testing.T) {
	cfg := rockbox.DefaultConfig()
	cfg.Volume = 0.0
	player, err := rockbox.NewPlayer(cfg)
	if err != nil {
		// Constructing a Player opens a live output device; headless CI
		// runners often have none. Skip rather than fail there.
		t.Skipf("no audio output device available: %v", err)
	}
	defer player.Close()

	if player.SampleRate() == 0 {
		t.Fatal("SampleRate should be > 0")
	}
	player.SetVolume(0.0)
	if err := player.SetQueue([]string{fixture(t)}); err != nil {
		t.Fatal(err)
	}
	time.Sleep(100 * time.Millisecond) // queue command is applied asynchronously

	st, err := player.Status()
	if err != nil {
		t.Fatal(err)
	}
	if st.State != "stopped" {
		t.Errorf("State = %q, want stopped", st.State)
	}
	if st.QueueLen != 1 {
		t.Errorf("QueueLen = %d, want 1", st.QueueLen)
	}
}
