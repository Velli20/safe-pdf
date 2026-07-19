require "json"

profile = JSON.parse(File.read(ARGV.fetch(0)))
points = profile.fetch("pps")
total = points.sum { |point| point.fetch("tb") }
total_blocks = points.sum { |point| point.fetch("tbk") }
peak_live = points.sum { |point| point.fetch("gb") }
peak_live_blocks = points.sum { |point| point.fetch("gbk") }
end_live = points.sum { |point| point.fetch("eb") }
end_live_blocks = points.sum { |point| point.fetch("ebk") }
frames = profile.fetch("ftbl")

def mib(bytes)
  format("%.2f", bytes / 1_048_576.0)
end

puts "total_allocated_bytes=#{total} total_allocated_mib=#{mib(total)} total_blocks=#{total_blocks}"
puts "peak_live_bytes=#{peak_live} peak_live_mib=#{mib(peak_live)} peak_live_blocks=#{peak_live_blocks}"
puts "end_live_bytes=#{end_live} end_live_mib=#{mib(end_live)} end_live_blocks=#{end_live_blocks}"

if profile.key?("tg")
  puts "profile_duration_#{profile.fetch("tu")}=#{profile.fetch("tg")}"
end

puts "allocation_site\ttotal_bytes\ttotal_mib\tblocks\tmax_live_bytes\tpercent_of_total\tproject_stack"
points.sort_by { |point| -point.fetch("tb") }.first(15).each do |point|
  stack = point.fetch("fs").map { |index| frames.fetch(index) }
  project_stack = stack.select do |frame|
    frame.include?("pdf_") || frame.include?("pdf-") || frame.include?("heap_profile")
  end.uniq
  site = project_stack.first || stack.first
  percent = total.zero? ? 0.0 : point.fetch("tb") * 100.0 / total
  puts [
    site,
    point.fetch("tb"),
    mib(point.fetch("tb")),
    point.fetch("tbk"),
    point.fetch("mb"),
    format("%.3f", percent),
    project_stack.first(8).join(" <- ")
  ].join("\t")
end
