//go:build darwin
// +build darwin

package hardware

import (
	"context"
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/shirou/gopsutil/v3/cpu"
	"github.com/shirou/gopsutil/v3/mem"
)

// DarwinMonitor implements HardwareMonitor for macOS
type DarwinMonitor struct {
	thresholds ResourceThresholds
}

// NewDarwinMonitor creates a new macOS hardware monitor
func NewDarwinMonitor(thresholds ResourceThresholds) *DarwinMonitor {
	return &DarwinMonitor{
		thresholds: thresholds,
	}
}

// GetCurrentResources returns the current system resource state
func (m *DarwinMonitor) GetCurrentResources() (*SystemResources, error) {
	resources := &SystemResources{
		Timestamp: time.Now(),
	}

	// Get RAM info using gopsutil
	vmStat, err := mem.VirtualMemory()
	if err != nil {
		return nil, fmt.Errorf("failed to get memory info: %w", err)
	}
	resources.TotalRAM = int64(vmStat.Total)
	resources.AvailableRAM = int64(vmStat.Available)

	// Get CPU usage
	cpuPercent, err := cpu.Percent(100*time.Millisecond, false)
	if err == nil && len(cpuPercent) > 0 {
		resources.CPUUsage = cpuPercent[0] / 100.0
	}

	// Get VRAM info for Apple Silicon (unified memory)
	vramInfo, err := m.getVRAMInfo()
	if err != nil {
		// On Apple Silicon with unified memory, VRAM = RAM
		// Use available RAM as a proxy for available VRAM
		resources.TotalVRAM = resources.TotalRAM
		resources.AvailableVRAM = resources.AvailableRAM
	} else {
		resources.TotalVRAM = vramInfo.total
		resources.AvailableVRAM = vramInfo.available
	}

	return resources, nil
}

// vramInfo holds VRAM information
type vramInfo struct {
	total     int64
	available int64
}

// getVRAMInfo attempts to get VRAM information using various methods
func (m *DarwinMonitor) getVRAMInfo() (*vramInfo, error) {
	// Method 1: Try ioreg for discrete GPU
	info, err := m.getVRAMViaIoreg()
	if err == nil {
		return info, nil
	}

	// Method 2: For Apple Silicon, use memory pressure as proxy
	// Apple Silicon uses unified memory, so VRAM = subset of RAM
	return m.getUnifiedMemoryInfo()
}

// getVRAMViaIoreg attempts to get VRAM info using ioreg command
func (m *DarwinMonitor) getVRAMViaIoreg() (*vramInfo, error) {
	// Try to find GPU VRAM information
	cmd := exec.Command("ioreg", "-r", "-c", "IOAccelerator")
	output, err := cmd.Output()
	if err != nil {
		return nil, err
	}

	outputStr := string(output)

	// Look for VRAM,totalMB or similar patterns
	vramRegex := regexp.MustCompile(`"VRAM,totalMB"\s*=\s*(\d+)`)
	matches := vramRegex.FindStringSubmatch(outputStr)
	if len(matches) > 1 {
		vramMB, err := strconv.ParseInt(matches[1], 10, 64)
		if err == nil {
			return &vramInfo{
				total:     vramMB * 1024 * 1024,
				available: vramMB * 1024 * 1024 / 2, // Estimate 50% available
			}, nil
		}
	}

	return nil, fmt.Errorf("VRAM info not found in ioreg output")
}

// getUnifiedMemoryInfo gets memory info for Apple Silicon unified memory
func (m *DarwinMonitor) getUnifiedMemoryInfo() (*vramInfo, error) {
	// Use memory_pressure command to get a better picture
	cmd := exec.Command("memory_pressure")
	output, err := cmd.Output()
	if err != nil {
		return nil, err
	}

	outputStr := string(output)

	// Parse memory pressure output
	// Example: "System-wide memory free percentage: 42%"
	freeRegex := regexp.MustCompile(`free percentage:\s*(\d+)%`)
	matches := freeRegex.FindStringSubmatch(outputStr)

	vmStat, _ := mem.VirtualMemory()
	total := int64(vmStat.Total)

	var freePercent int64 = 50 // Default
	if len(matches) > 1 {
		freePercent, _ = strconv.ParseInt(matches[1], 10, 64)
	}

	available := total * freePercent / 100

	return &vramInfo{
		total:     total,
		available: available,
	}, nil
}

// CanRunCapsule checks if there are enough resources to run a Capsule
func (m *DarwinMonitor) CanRunCapsule(vramRequired int64) (*ResourceCheckResult, error) {
	resources, err := m.GetCurrentResources()
	if err != nil {
		return nil, fmt.Errorf("failed to get resources: %w", err)
	}

	result := &ResourceCheckResult{
		CanRun: true,
	}

	vramUsage := resources.VRAMUsagePercent()
	ramUsage := resources.RAMUsagePercent()

	// Check VRAM warning threshold
	if vramUsage >= m.thresholds.VRAMWarningPercent {
		result.VRAMWarning = true
	}

	// Check RAM warning threshold
	if ramUsage >= m.thresholds.RAMWarningPercent {
		result.RAMWarning = true
	}

	// Check if we're at the block threshold
	if vramUsage >= m.thresholds.VRAMBlockPercent {
		result.CanRun = false
		result.Reason = fmt.Sprintf("VRAM usage too high (%.1f%% >= %.1f%%)",
			vramUsage, m.thresholds.VRAMBlockPercent)
		return result, nil
	}

	// Check if we have enough VRAM for this specific Capsule
	if vramRequired > 0 && resources.AvailableVRAM < vramRequired {
		result.CanRun = false
		result.Reason = fmt.Sprintf("Insufficient VRAM: need %.2fGB, have %.2fGB available",
			float64(vramRequired)/(1024*1024*1024),
			resources.AvailableVRAMGB())
		return result, nil
	}

	return result, nil
}

// Watch starts continuous monitoring
func (m *DarwinMonitor) Watch(ctx context.Context, interval time.Duration, callback func(*SystemResources)) {
	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	// Initial call
	if resources, err := m.GetCurrentResources(); err == nil {
		callback(resources)
	}

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			if resources, err := m.GetCurrentResources(); err == nil {
				callback(resources)
			}
		}
	}
}

// GetSystemSummary returns a human-readable summary of system resources
func (m *DarwinMonitor) GetSystemSummary() (string, error) {
	resources, err := m.GetCurrentResources()
	if err != nil {
		return "", err
	}

	var sb strings.Builder
	sb.WriteString(fmt.Sprintf("System Resources:\n"))
	sb.WriteString(fmt.Sprintf("  RAM:  %.1f GB / %.1f GB (%.1f%% used)\n",
		float64(resources.TotalRAM-resources.AvailableRAM)/(1024*1024*1024),
		float64(resources.TotalRAM)/(1024*1024*1024),
		resources.RAMUsagePercent()))
	sb.WriteString(fmt.Sprintf("  VRAM: %.1f GB / %.1f GB (%.1f%% used)\n",
		float64(resources.TotalVRAM-resources.AvailableVRAM)/(1024*1024*1024),
		float64(resources.TotalVRAM)/(1024*1024*1024),
		resources.VRAMUsagePercent()))
	sb.WriteString(fmt.Sprintf("  CPU:  %.1f%% used\n", resources.CPUUsage*100))

	return sb.String(), nil
}
