/**
 * Parse tool calls from LLM responses in the MCP format.
 * This is similar to the ToolParser in the Rust implementation.
 */

// UUID import removed

export interface ParsedToolCall {
  // ID field removed - it will be handled by LangChain's AIMessage.tool_calls
  name: string;
  arguments: Record<string, any>;
  fullText: string; // The full text of the tool call including delimiters
}

export class ToolParser {
  private static readonly TOOL_CALL_START = '<<<TOOL_CALL>>>';
  private static readonly TOOL_CALL_END = '<<<END_TOOL_CALL>>>';

  /**
   * Parse tool calls from a text response.
   * @param text The text to parse tool calls from.
   * @returns Array of parsed tool calls.
   */
  static parseToolCalls(text: string): ParsedToolCall[] {
    const toolCalls: ParsedToolCall[] = [];
    let currentPos = 0;

    while (currentPos < text.length) {
      // Find the start of a tool call
      const startIndex = text.indexOf(this.TOOL_CALL_START, currentPos);
      if (startIndex === -1) break; // No more tool calls found

      // Find the end of the tool call
      const endIndex = text.indexOf(this.TOOL_CALL_END, startIndex);
      if (endIndex === -1) {
          console.warn(`[ToolParser] Found start delimiter but no end delimiter after position ${startIndex}.`);
          break; // No end delimiter found for this start
      }

      // Extract the potential JSON content between the delimiters
      const jsonStart = startIndex + this.TOOL_CALL_START.length;
      const jsonEnd = endIndex;
      const potentialJsonContent = text.substring(jsonStart, jsonEnd).trim();

      // Attempt to parse the JSON content
      try {
        const toolCallData = JSON.parse(potentialJsonContent);
        
        // Validate structure (name and arguments fields)
        if (
          typeof toolCallData === 'object' && 
          toolCallData !== null &&
          typeof toolCallData.name === 'string' && 
          typeof toolCallData.arguments === 'object' && 
          toolCallData.arguments !== null && 
          !Array.isArray(toolCallData.arguments)
        ) {
          // ID is no longer generated or stored here
          toolCalls.push({
            name: toolCallData.name,
            arguments: toolCallData.arguments,
            // Capture the actual text including delimiters for this parsed call
            fullText: text.substring(startIndex, endIndex + this.TOOL_CALL_END.length)
          });
        } else {
          console.warn('[ToolParser] Invalid tool call JSON structure:', potentialJsonContent);
        }
      } catch (error) {
        // If JSON parsing fails, it might be due to nested delimiters or invalid JSON.
        // Log the error but continue searching from *after* the start delimiter
        // to potentially find later valid calls. This isn't perfect for nesting.
        console.warn(`[ToolParser] Error parsing tool call JSON between indices ${jsonStart} and ${jsonEnd}:`, error instanceof Error ? error.message : String(error));
        console.warn('[ToolParser] Raw content:', potentialJsonContent);
      }

      // Move search position past the *current end delimiter* to find the next potential start
      currentPos = endIndex + this.TOOL_CALL_END.length;
    }

    return toolCalls;
  }

  /**
   * Check if a response contains any tool calls.
   * @param text The text to check.
   * @returns True if the text contains tool calls, false otherwise.
   */
  static containsToolCalls(text: string): boolean {
    return text.includes(this.TOOL_CALL_START) && text.includes(this.TOOL_CALL_END);
  }

  /**
   * Replace tool calls in text with a placeholder and extract them.
   * @param text The text containing tool calls.
   * @returns Object with the text with tool calls replaced and the extracted tool calls.
   */
  static extractAndReplace(text: string): { 
    cleanText: string; 
    toolCalls: ParsedToolCall[] 
  } {
    let cleanText = text;
    const toolCalls = this.parseToolCalls(text);

    // Replace each tool call with a placeholder
    for (const toolCall of toolCalls) {
      cleanText = cleanText.replace(toolCall.fullText, `[Tool Call: ${toolCall.name}]`);
    }

    return { cleanText, toolCalls };
  }
}
