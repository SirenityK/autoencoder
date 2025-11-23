package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"

	"github.com/spf13/cobra"
)

// --- Types & Config ---

type Options struct {
	Preset        string
	Tune          string
	Output        string
	Resolution    int
	VideoBitrate  string
	AudioBitrate  string
	TwoPass       bool
	CRF           int
	EncoderPreset string // New flag for speed/quality preset (e.g. "slow", "5")
}

type Dimensions struct {
	Width  int
	Height int
}

var (
	opts      Options
	hasLibFdk bool
)

// --- Main Entry Point ---

func main() {
	hasLibFdk = checkLibFdk()

	rootCmd := &cobra.Command{
		Use:   "autoencode [input_files...]",
		Short: "Intelligent video encoder for WhatsApp and Discord",
		Long:  "A binary CLI tool to replace complex FFmpeg bash scripts.",
		Args:  cobra.MinimumNArgs(1),
		ValidArgsFunction: func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
			return nil, cobra.ShellCompDirectiveDefault
		},
		Run: runRoot,
	}

	setupFlags(rootCmd)

	if err := rootCmd.Execute(); err != nil {
		fmt.Println(err)
		os.Exit(1)
	}
}

func setupFlags(cmd *cobra.Command) {
	cmd.PersistentFlags().StringVarP(&opts.Preset, "preset", "p", "whatsapp", "Encoder preset: 'whatsapp' or 'discord'")
	cmd.PersistentFlags().StringVarP(&opts.Tune, "tune", "t", "", "FFmpeg tune option. Only applies to whatsapp preset.")
	cmd.PersistentFlags().StringVarP(&opts.Output, "output", "o", "", "Output filename (optional)")
	cmd.PersistentFlags().IntVarP(&opts.Resolution, "resolution", "r", 480, "Target resolution")
	cmd.PersistentFlags().StringVarP(&opts.VideoBitrate, "vbitrate", "b", "", "Target video bitrate (e.g., '1000k')")
	cmd.PersistentFlags().StringVarP(&opts.AudioBitrate, "abitrate", "a", "", "Target audio bitrate (e.g., '128k')")
	cmd.PersistentFlags().IntVarP(&opts.CRF, "crf", "c", 28, "CRF value (default 28). Only applies to whatsapp preset.")
	cmd.PersistentFlags().BoolVar(&opts.TwoPass, "two-pass", false, "Enable two-pass encoding")
	cmd.PersistentFlags().StringVarP(&opts.EncoderPreset, "enc-preset", "e", "", "Encoder speed/quality preset (e.g., 'slow' for whatsapp, '5' for discord)")

	// Completions
	cmd.RegisterFlagCompletionFunc("preset", func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
		return []string{"whatsapp", "discord"}, cobra.ShellCompDirectiveNoFileComp
	})
	cmd.RegisterFlagCompletionFunc("tune", func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
		return []string{"film", "animation", "grain", "stillimage", "fastdecode", "zerolatency"}, cobra.ShellCompDirectiveNoFileComp
	})

	cmd.AddCommand(completionCmd)
}

var completionCmd = &cobra.Command{
	Use:       "completion [bash|zsh|fish|powershell]",
	Short:     "Generate completion script",
	ValidArgs: []string{"bash", "zsh", "fish", "powershell"},
	Args:      cobra.ExactArgs(1),
	Run: func(cmd *cobra.Command, args []string) {
		switch args[0] {
		case "bash":
			cmd.Root().GenBashCompletion(os.Stdout)
		case "zsh":
			cmd.Root().GenZshCompletion(os.Stdout)
		case "fish":
			cmd.Root().GenFishCompletion(os.Stdout, true)
		case "powershell":
			cmd.Root().GenPowerShellCompletionWithDesc(os.Stdout)
		}
	},
}

// --- Orchestration ---

func runRoot(cmd *cobra.Command, args []string) {
	for _, inputFile := range args {
		if err := processVideo(inputFile); err != nil {
			fmt.Fprintf(os.Stderr, "Error processing %s: %v\n", inputFile, err)
		}
	}
}

func processVideo(inputFile string) error {
	fmt.Printf("Processing: %s using preset [%s]\n", inputFile, opts.Preset)

	// 1. Analyze
	dims, err := getVideoDimensions(inputFile)
	if err != nil {
		return fmt.Errorf("failed to probe video: %w", err)
	}

	// 2. Strategize (Scale & Naming)
	vf := buildVideoFilters(dims, opts.Resolution)
	outputFile := resolveOutputFilename(inputFile, opts.Output, opts.Preset)

	// 3. Execute
	switch opts.Preset {
	case "whatsapp":
		return encodeWhatsapp(inputFile, outputFile, vf)
	case "discord":
		return encodeDiscord(inputFile, outputFile, vf)
	default:
		return fmt.Errorf("unknown preset: %s", opts.Preset)
	}
}

// --- Core Logic: Whatsapp Preset ---

func encodeWhatsapp(input, output, vf string) error {
	audioArgs := getWhatsappAudioArgs()

	// Construct Base Args (Input + Filters + Audio)
	baseArgs := []string{"-i", input, "-vf", vf, "-sws_flags", "lanczos"}
	baseArgs = append(baseArgs, audioArgs...)

	// Determine Video Strategy
	if opts.TwoPass {
		return runWhatsappTwoPass(baseArgs, output, input)
	}
	return runWhatsappSinglePass(baseArgs, output)
}

func runWhatsappSinglePass(baseArgs []string, output string) error {
	// Default to "slow" if not provided
	encPreset := opts.EncoderPreset
	if encPreset == "" {
		encPreset = "slow"
	}

	videoArgs := []string{
		"-c:v", "libx264",
		"-preset", encPreset,
		"-movflags", "+faststart",
	}

	// Apply standard color/format flags
	videoArgs = append(videoArgs, getStandardFlags()...)

	if opts.VideoBitrate != "" {
		videoArgs = append(videoArgs, "-b:v", opts.VideoBitrate)
	} else {
		videoArgs = append(videoArgs, "-crf", strconv.Itoa(opts.CRF))
	}

	if opts.Tune != "" {
		videoArgs = append(videoArgs, "-tune", opts.Tune)
	}

	finalArgs := append([]string{"-y"}, baseArgs...)
	finalArgs = append(finalArgs, videoArgs...)
	finalArgs = append(finalArgs, output)

	fmt.Println("Running FFmpeg (Whatsapp Single Pass)...")
	return runCommand("ffmpeg", finalArgs...)
}

func runWhatsappTwoPass(baseArgs []string, output string, inputPath string) error {
	// Default to "slow" if not provided
	encPreset := opts.EncoderPreset
	if encPreset == "" {
		encPreset = "slow"
	}

	// Setup Logs
	passLog := fmt.Sprintf("ffmpeg2pass-%s", filepath.Base(inputPath))
	defer func() {
		os.Remove(passLog + "-0.log")
		os.Remove(passLog + "-0.log.mbtree")
	}()

	targetBitrate := "1000k" // Default 480p bitrate
	if opts.VideoBitrate != "" {
		targetBitrate = opts.VideoBitrate
	}

	commonVideoArgs := []string{
		"-c:v", "libx264",
		"-preset", encPreset,
		"-movflags", "+faststart",
		"-b:v", targetBitrate,
	}

	// Apply standard color/format flags (Needed for both passes to match geometry/color)
	commonVideoArgs = append(commonVideoArgs, getStandardFlags()...)

	if opts.Tune != "" {
		commonVideoArgs = append(commonVideoArgs, "-tune", opts.Tune)
	}

	// --- Pass 1 ---
	// Pass 1 needs -an (no audio) and -f null
	pass1Base := filterAudioFlags(baseArgs)
	pass1Args := append([]string{"-y"}, pass1Base...)
	pass1Args = append(pass1Args, "-an")
	pass1Args = append(pass1Args, commonVideoArgs...)
	pass1Args = append(pass1Args, "-pass", "1", "-passlogfile", passLog, "-f", "null", nullDevice())

	fmt.Println("Running FFmpeg (Whatsapp 2-Pass): Pass 1...")
	if err := runCommand("ffmpeg", pass1Args...); err != nil {
		return fmt.Errorf("pass 1 failed: %w", err)
	}

	// --- Pass 2 ---
	pass2Args := append([]string{"-y"}, baseArgs...)
	pass2Args = append(pass2Args, commonVideoArgs...)
	pass2Args = append(pass2Args, "-pass", "2", "-passlogfile", passLog, output)

	fmt.Println("Running FFmpeg (Whatsapp 2-Pass): Pass 2...")
	return runCommand("ffmpeg", pass2Args...)
}

func getWhatsappAudioArgs() []string {
	acodec := "aac"
	abitrate := "96k"

	if hasLibFdk {
		acodec = "libfdk_aac"
		abitrate = "32k"
	}
	if opts.AudioBitrate != "" {
		abitrate = opts.AudioBitrate
	}

	args := []string{
		"-c:a", acodec,
		"-b:a", abitrate,
		"-ac", "2",
	}

	if acodec == "libfdk_aac" {
		args = append(args, "-profile:a", "aac_he_v2")
	}
	return args
}

// --- Core Logic: Discord Preset ---

func encodeDiscord(input, output, vf string) error {
	statsFile := "svtav1_stats.stat"
	tempIvf := "temp_video.ivf"
	defer os.Remove(statsFile)
	defer os.Remove(tempIvf)

	vBitrate := "256k"
	if opts.VideoBitrate != "" {
		vBitrate = opts.VideoBitrate
	}

	// Default to "5" if not provided
	encPreset := opts.EncoderPreset
	if encPreset == "" {
		encPreset = "5"
	}

	// Pass 1
	fmt.Println("Running SVT-AV1 Pass 1...")
	p1Cmd := buildSvtCmd(input, vf, vBitrate, encPreset, statsFile, "", 1)
	if err := runCommand("sh", "-c", p1Cmd); err != nil {
		return fmt.Errorf("pass 1 failed: %w", err)
	}

	// Pass 2
	fmt.Println("Running SVT-AV1 Pass 2...")
	p2Cmd := buildSvtCmd(input, vf, vBitrate, encPreset, statsFile, tempIvf, 2)
	if err := runCommand("sh", "-c", p2Cmd); err != nil {
		return fmt.Errorf("pass 2 failed: %w", err)
	}

	// Muxing
	fmt.Println("Muxing Audio and Video...")
	aBitrate := "32k"
	if opts.AudioBitrate != "" {
		aBitrate = opts.AudioBitrate
	}

	return runCommand("ffmpeg",
		"-i", tempIvf,
		"-i", input,
		"-map", "0:v",
		"-map", "1:a",
		"-c:v", "copy",
		"-c:a", "libopus",
		"-b:a", aBitrate,
		"-ac", "2",
		"-y", output,
	)
}

func buildSvtCmd(input, vf, bitrate, preset, stats, output string, pass int) string {
	// Combine standard flags into a string to inject into the command
	stdFlags := strings.Join(getStandardFlags(), " ")

	// We inject stdFlags into the ffmpeg part to ensure the pipe receives correct yuv420p/bt709 data
	base := fmt.Sprintf("ffmpeg -i \"%s\" -map 0:v -vf \"%s\" %s -f yuv4mpegpipe - | SvtAv1EncApp -i - --preset %s --rc 1 --tbr %s --stats \"%s\" --pass %d",
		input, vf, stdFlags, preset, bitrate, stats, pass)

	if pass == 2 {
		base += fmt.Sprintf(" -b \"%s\"", output)
	}
	return base
}

// --- Helpers ---

// getStandardFlags returns the FFmpeg flags required to standardize the video
// to bt709 color space and yuv420p pixel format.
func getStandardFlags() []string {
	return []string{
		"-pix_fmt", "yuv420p",
		"-colorspace", "bt709",
		"-color_trc", "bt709",
		"-color_primaries", "bt709",
		"-color_range", "tv",
	}
}

func buildVideoFilters(dims Dimensions, resolution int) string {
	res := strconv.Itoa(resolution)
	scale := ""
	if dims.Height > dims.Width {
		scale = fmt.Sprintf("scale=%s:-2", res)
	} else {
		scale = fmt.Sprintf("scale=-2:%s", res)
	}
	return fmt.Sprintf("fps=min(ntsc\\,source_fps),%s", scale)
}

func resolveOutputFilename(input, outputFlag, preset string) string {
	if outputFlag != "" {
		return outputFlag
	}
	ext := filepath.Ext(input)
	name := strings.TrimSuffix(input, ext)
	return fmt.Sprintf("%s_%s.mp4", name, preset)
}

func getVideoDimensions(input string) (Dimensions, error) {
	out, err := exec.Command("ffprobe",
		"-v", "error",
		"-select_streams", "v:0",
		"-show_entries", "stream=width,height",
		"-of", "json",
		input,
	).Output()

	if err != nil {
		return Dimensions{}, err
	}

	var probeData struct {
		Streams []struct {
			Width  int `json:"width"`
			Height int `json:"height"`
		} `json:"streams"`
	}

	if err := json.Unmarshal(out, &probeData); err != nil {
		return Dimensions{}, err
	}

	if len(probeData.Streams) == 0 {
		return Dimensions{}, fmt.Errorf("no video streams found")
	}

	return Dimensions{
		Width:  probeData.Streams[0].Width,
		Height: probeData.Streams[0].Height,
	}, nil
}

func runCommand(name string, args ...string) error {
	cmd := exec.Command(name, args...)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

func checkLibFdk() bool {
	cmd := exec.Command("ffmpeg", "-encoders")
	out, err := cmd.Output()
	if err != nil {
		return false
	}
	return strings.Contains(string(out), "libfdk_aac")
}

func nullDevice() string {
	if runtime.GOOS == "windows" {
		return "NUL"
	}
	return "/dev/null"
}

func filterAudioFlags(args []string) []string {
	var filtered []string
	skipNext := false
	for _, arg := range args {
		if skipNext {
			skipNext = false
			continue
		}
		if arg == "-c:a" || arg == "-b:a" || arg == "-profile:a" || arg == "-ac" {
			skipNext = true
			continue
		}
		filtered = append(filtered, arg)
	}
	return filtered
}
