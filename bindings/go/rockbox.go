package rockbox

import "math"

// ABIVersion reports the ABI major version of the loaded library (bumped on
// breaking changes).
func ABIVersion() uint32 {
	return rbFFIABIVersion()
}

// SineStereo generates seconds of a sine at freqHz as interleaved stereo int16
// samples at the given sample rate. amplitude is the peak (e.g. 16000).
func SineStereo(freqHz float64, seconds float64, rate int, amplitude int) []int16 {
	n := int(seconds * float64(rate))
	buf := make([]int16, n*2)
	for i := 0; i < n; i++ {
		s := int(math.Sin(float64(i)*2.0*math.Pi*freqHz/float64(rate)) * float64(amplitude))
		if s < math.MinInt16 {
			s = math.MinInt16
		}
		if s > math.MaxInt16 {
			s = math.MaxInt16
		}
		buf[i*2] = int16(s)
		buf[i*2+1] = int16(s)
	}
	return buf
}
