                                             
 use regex::Regex;
 use shared_protocol_objects::ToolInfo;
 
 /// Build a dynamic regex that references each known tool name from the server.
 pub fn build_tool_call_regex(tool_names: &[String]) -> Regex {
     // Convert each name into a safe, escaped pattern. Then join with `|` so we can match any of them
     let escaped_names: Vec<String> = tool_names
         .iter()
         .map(|tn| regex::escape(tn))    // e.g. "brave_search" => "brave_search"
         .collect();
     // e.g. "brave_search|graph_tool|git|..."
     let names_alt = escaped_names.join("|");
 
     // Build a pattern like:
     // (?:Let me call|I(?:'|’)ll use|Using the)
     // \s+`?(brave_search|graph_tool|git|...)`?\s*(?:tool)?
     // .*?
     // (?:```(?:json)?\s*)?
     // (\{)
     let pattern = format!(
         r"(?sx)
          (?:Let\ me\ call|I(?:'|’)ll\ use|Using\ the)
          \s+`?({})`?\s*(?:tool)?
          .*?
          (?:```(?:json)?\s*)?
          (\{{)
         ",
         names_alt
     );
 
     Regex::new(&pattern).expect("Failed to build dynamic MASTER_REGEX")
 }
