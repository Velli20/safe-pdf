def median(values)
  sorted = values.sort
  middle = sorted.length / 2
  return sorted.fetch(middle) if sorted.length.odd?

  (sorted.fetch(middle - 1) + sorted.fetch(middle)) / 2.0
end

groups = {}

ARGV.each do |path|
  text = File.read(path)
  modes = text.scan(/mode=([^ ]+) [^\n]*elapsed_ns=(\d+)/)
  next if modes.empty?

  mode = modes.first.fetch(0)
  group = groups[mode] ||= {
    times: [],
    rss: [],
    instructions: [],
    cycles: []
  }
  group[:times].concat(modes.map { |_, value| Integer(value) })
  mac_rss = text.scan(/(\d+)\s+maximum resident set size/).flatten.map { |v| Integer(v) }
  linux_rss = text.scan(/Maximum resident set size \(kbytes\):\s*(\d+)/i)
    .flatten
    .map { |v| Integer(v) * 1024 }
  group[:rss].concat(mac_rss).concat(linux_rss)
  group[:instructions].concat(text.scan(/(\d+)\s+instructions retired/).flatten.map { |v| Integer(v) })
  group[:cycles].concat(text.scan(/(\d+)\s+cycles elapsed/).flatten.map { |v| Integer(v) })
end

puts "mode\truns\tmedian_ms\tmin_ms\tmax_ms\tprocess_peak_rss_mib\tprocess_median_instructions\tprocess_median_cycles"
preferred_modes = ["io", "parse", "end-to-end"]
ordered_modes = preferred_modes.select { |mode| groups.key?(mode) } + (groups.keys - preferred_modes).sort
ordered_modes.each do |mode|
  group = groups.fetch(mode)
  times = group.fetch(:times)
  rss = group.fetch(:rss)
  instructions = group.fetch(:instructions)
  cycles = group.fetch(:cycles)
  puts [
    mode,
    times.length,
    format("%.3f", median(times) / 1_000_000.0),
    format("%.3f", times.min / 1_000_000.0),
    format("%.3f", times.max / 1_000_000.0),
    rss.empty? ? "unavailable" : format("%.2f", rss.max / 1_048_576.0),
    instructions.empty? ? "unavailable" : median(instructions).round,
    cycles.empty? ? "unavailable" : median(cycles).round
  ].join("\t")
end
