import Foundation

struct Completion: Identifiable {
    var text: String
    var detail: String
    var id: String { text }
}
struct MarkdownCell: Identifiable {
    var id: Int
    var text: String
}
struct MarkdownBlock: Identifiable {
    var id: Int
    var text: String
    var kind: String
}

/// Pure text helpers shared by both platforms and covered by native contract tests.
enum MessageText {
    static func characters(_ text: String) -> [Character] {
        #if SKIP
        // SKIP REPLACE: return Array(text.toList())
        return []
        #else
        return Array(text)
        #endif
    }
    static func word(_ c: Character) -> Bool {
        #if SKIP
        // SKIP REPLACE: return c.isLetterOrDigit() || c == '_' || c == '-'
        return false
        #else
        return c.isLetter || c.isNumber || c == "_" || c == "-"
        #endif
    }
    static func suggestions(_ draft: String, users: [Person], channels: [Channel]) -> [Completion] {
        let chars = characters(draft)
        var start = chars.count
        while start > 0 && word(chars[start - 1]) { start -= 1 }
        guard start > 0 else { return [] }
        let marker = chars[start - 1]
        guard marker == "@" || marker == "#" else { return [] }
        if start > 1 && !chars[start - 2].isWhitespace && !"([{>".contains(chars[start - 2]) { return [] }
        let query = String(chars[start...]).lowercased()
        var result: [Completion] = []
        if marker == "@" {
            for user in users {
                if let handle = user.handle, query.isEmpty || handle.lowercased().contains(query) || user.displayName.lowercased().contains(query) {
                    result.append(Completion(text: "@\(handle)", detail: user.displayName))
                }
            }
        } else {
            for channel in channels where !channel.isDirect && !channel.isSaved && (query.isEmpty || channel.name.lowercased().contains(query)) {
                result.append(Completion(text: "#\(channel.name)", detail: channel.isPrivate ? "Private channel" : channel.topic ?? "Channel"))
            }
        }
        result.sort { a, b in
            let ap = String(a.text.dropFirst()).lowercased().hasPrefix(query)
            let bp = String(b.text.dropFirst()).lowercased().hasPrefix(query)
            return ap == bp ? a.text.lowercased() < b.text.lowercased() : ap
        }
        return Array(result.prefix(8))
    }
    static func complete(_ draft: String, with text: String) -> String {
        let chars = characters(draft)
        var start = chars.count
        while start > 0 && word(chars[start - 1]) { start -= 1 }
        if start > 0 && (chars[start - 1] == "@" || chars[start - 1] == "#") { start -= 1 }
        return String(chars.prefix(start)) + text + " "
    }
    static func cells(_ text: String) -> [MarkdownCell] {
        var result: [MarkdownCell] = []
        for cell in text.components(separatedBy: "|").dropFirst().dropLast() { result.append(MarkdownCell(id: result.count, text: cell)) }
        return result
    }
    static func blocks(_ text: String) -> [MarkdownBlock] {
        let lines = text.components(separatedBy: "\n")
        var result: [MarkdownBlock] = []
        var buffer: [String] = []
        var code = false
        var language = ""
        for line in lines {
            if line.hasPrefix("```") || line.hasPrefix("~~~") {
                if !buffer.isEmpty { result.append(MarkdownBlock(id: result.count, text: buffer.joined(separator: "\n"), kind: code ? "code:" + language : "text")); buffer = [] }
                code.toggle()
                language = String(line.dropFirst(3)).trimmingCharacters(in: .whitespaces)
            } else if !code, var special = specialBlock(line) {
                if !buffer.isEmpty { result.append(MarkdownBlock(id: result.count, text: buffer.joined(separator: "\n"), kind: "text")); buffer = [] }
                special.id = result.count
                result.append(special)
            } else if !code && line.isEmpty {
                if !buffer.isEmpty { result.append(MarkdownBlock(id: result.count, text: buffer.joined(separator: "\n"), kind: "text")); buffer = [] }
            } else { buffer.append(line) }
        }
        if !buffer.isEmpty { result.append(MarkdownBlock(id: result.count, text: buffer.joined(separator: "\n"), kind: code ? "code:" + language : "text")) }
        return result
    }
    private static func specialBlock(_ line: String) -> MarkdownBlock? {
        if ["---", "***", "___"].contains(line.trimmingCharacters(in: .whitespaces)) { return MarkdownBlock(id: 0, text: "", kind: "rule") }
        let chars = characters(line)
        var hashes = 0
        while hashes < chars.count && chars[hashes] == "#" { hashes += 1 }
        if hashes > 0 && hashes <= 6 && hashes < chars.count && chars[hashes] == " " {
            return MarkdownBlock(id: 0, text: String(chars.dropFirst(hashes + 1)), kind: "heading" + String(hashes))
        }
        if line.hasPrefix("> ") { return MarkdownBlock(id: 0, text: String(line.dropFirst(2)), kind: "quote") }
        if line.hasPrefix("- [ ] ") { return MarkdownBlock(id: 0, text: "☐ " + String(line.dropFirst(6)), kind: "list") }
        if line.hasPrefix("- [x] ") || line.hasPrefix("- [X] ") { return MarkdownBlock(id: 0, text: "☑ " + String(line.dropFirst(6)), kind: "list") }
        if line.hasPrefix("- ") || line.hasPrefix("* ") { return MarkdownBlock(id: 0, text: "• " + String(line.dropFirst(2)), kind: "list") }
        if line.hasPrefix("|") && line.hasSuffix("|") {
            let divider = line.replacingOccurrences(of: "|", with: "").replacingOccurrences(of: "-", with: "").replacingOccurrences(of: ":", with: "").trimmingCharacters(in: .whitespaces)
            return MarkdownBlock(id: 0, text: line, kind: divider.isEmpty ? "tableRule" : "table")
        }
        return nil
    }
    static func linked(_ text: String, workspace: String?, users: [Person], channels: [Channel]) -> String {
        guard let workspace else { return text }
        var refs: [Completion] = []
        for user in users { if let handle = user.handle { refs.append(Completion(text: "@" + handle, detail: "user/" + user.id)) } }
        for channel in channels where !channel.isDirect && !channel.isSaved { refs.append(Completion(text: "#" + channel.name, detail: "channel/" + channel.id)) }
        refs.sort { $0.text.count > $1.text.count }
        let chars = characters(text)
        var result = ""
        var index = 0
        var ticks = 0
        var bracket = 0
        var destination = false
        while index < chars.count {
            let c = chars[index]
            if c == "\\" && index + 1 < chars.count { result += String(chars[index...index + 1]); index += 2; continue }
            if c == "`" { ticks = ticks == 0 ? 1 : 0 }
            if ticks == 0 {
                if c == "[" { bracket += 1 }
                if c == "]" && bracket > 0 { bracket -= 1; if index + 1 < chars.count && chars[index + 1] == "(" { destination = true } }
                if c == ")" { destination = false }
            }
            if ticks == 0 && bracket == 0 && !destination && (c == "@" || c == "#") && (index == 0 || chars[index - 1].isWhitespace || "([{>".contains(chars[index - 1])) {
                var found: Completion?
                for ref in refs {
                    let end = index + ref.text.count
                    if end <= chars.count && String(chars[index..<end]).lowercased() == ref.text.lowercased() && (end == chars.count || !word(chars[end])) { found = ref; break }
                }
                if let ref = found {
                    let end = index + ref.text.count
                    let label = String(chars[index..<end]).replacingOccurrences(of: "[", with: "\\[").replacingOccurrences(of: "]", with: "\\]")
                    result += "[\(label)](horobets://workspace/\(workspace)/\(ref.detail))"
                    index = end; continue
                }
            }
            result += String(c); index += 1
        }
        return result
    }
}

extension MessageText {
    static func time(_ timestamp: Int64) -> String {
        let formatter = DateFormatter(); formatter.dateFormat = "HH:mm"
        return formatter.string(from: Date(timeIntervalSince1970: Double(timestamp) / 1000))
    }
    static func day(_ timestamp: Int64) -> String {
        let formatter = DateFormatter(); formatter.dateFormat = "EEE, MMM d, yyyy"
        let value = formatter.string(from: Date(timeIntervalSince1970: Double(timestamp) / 1000))
        if value == formatter.string(from: Date()) { return "Today" }
        if value == formatter.string(from: Date().addingTimeInterval(-86400)) { return "Yesterday" }
        return value
    }
    static func grouped(_ previous: Message, _ current: Message) -> Bool {
        previous.userId == current.userId && current.createdAt >= previous.createdAt && current.createdAt - previous.createdAt < 300_000 && day(previous.createdAt) == day(current.createdAt)
    }
}
