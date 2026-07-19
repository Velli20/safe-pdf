require "rexml/document"
require "rexml/xpath"

document = REXML::Document.new(File.read(ARGV.fetch(0)))
frame_names = {}
backtraces = {}
leaf_counts = Hash.new(0)
inclusive_counts = Hash.new(0)
symbolicated = 0
rows = REXML::XPath.match(document, "//row")

rows.each do |row|
  backtrace = row.elements["backtrace"]
  next unless backtrace

  stack = if backtrace.attributes["ref"]
    backtraces.fetch(backtrace.attributes["ref"])
  else
    names = backtrace.get_elements("frame").map do |frame|
      if frame.attributes["name"]
        frame_names[frame.attributes["id"]] = frame.attributes["name"]
      elsif frame.attributes["ref"]
        frame_names[frame.attributes["ref"]]
      end
    end.compact
    backtraces[backtrace.attributes["id"]] = names
  end

  next if stack.empty?
  symbolicated += 1
  leaf_counts[stack.first] += 1
  stack.uniq.each { |name| inclusive_counts[name] += 1 }
end
reader_entry = inclusive_counts.keys.find { |name| name.include?("PdfReader::read_with_report") }
reader_samples = reader_entry ? inclusive_counts.fetch(reader_entry) : symbolicated

puts "rows=#{rows.length} symbolicated=#{symbolicated} reader_samples=#{reader_samples}"
puts "top_leaf_samples\tcount\tpercent_of_reader"
leaf_counts.sort_by { |_, count| -count }.first(20).each do |name, count|
  percent = reader_samples.zero? ? 0.0 : count * 100.0 / reader_samples
  puts "#{name}\t#{count}\t#{format("%.2f", percent)}"
end
puts "top_inclusive_samples\tcount\tpercent_of_reader"
inclusive_counts.sort_by { |_, count| -count }.first(30).each do |name, count|
  percent = reader_samples.zero? ? 0.0 : count * 100.0 / reader_samples
  puts "#{name}\t#{count}\t#{format("%.2f", percent)}"
end
