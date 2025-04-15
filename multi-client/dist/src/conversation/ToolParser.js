/**
 * Parse tool calls from LLM responses in the MCP format.
 * This is similar to the ToolParser in the Rust implementation.
 */
export class ToolParser {
    static TOOL_CALL_START = '<<<TOOL_CALL>>>';
    static TOOL_CALL_END = '<<<END_TOOL_CALL>>>';
    /**
     * Parse tool calls from a text response.
     * @param text The text to parse tool calls from.
     * @returns Array of parsed tool calls.
     */
    static parseToolCalls(text) {
        const toolCalls = [];
        let currentPos = 0;
        while (currentPos < text.length) {
            // Find the start of a tool call
            const startIndex = text.indexOf(this.TOOL_CALL_START, currentPos);
            if (startIndex === -1)
                break; // No more tool calls found
            // Find the end of the tool call
            const endIndex = text.indexOf(this.TOOL_CALL_END, startIndex);
            if (endIndex === -1)
                break; // No end delimiter found
            // Extract the full tool call text
            const fullText = text.substring(startIndex, endIndex + this.TOOL_CALL_END.length);
            // Extract the JSON content between the delimiters
            const jsonStart = startIndex + this.TOOL_CALL_START.length;
            const jsonEnd = endIndex;
            const jsonContent = text.substring(jsonStart, jsonEnd).trim();
            try {
                // Parse the JSON content
                const toolCallData = JSON.parse(jsonContent);
                // Validate structure (name and arguments fields)
                if (typeof toolCallData === 'object' &&
                    toolCallData !== null &&
                    typeof toolCallData.name === 'string' &&
                    typeof toolCallData.arguments === 'object' &&
                    toolCallData.arguments !== null &&
                    !Array.isArray(toolCallData.arguments)) {
                    toolCalls.push({
                        name: toolCallData.name,
                        arguments: toolCallData.arguments,
                        fullText: fullText
                    });
                }
                else {
                    console.warn('Invalid tool call structure:', jsonContent);
                }
            }
            catch (error) {
                console.warn('Error parsing tool call JSON:', error instanceof Error ? error.message : String(error));
            }
            // Move past this tool call
            currentPos = endIndex + this.TOOL_CALL_END.length;
        }
        return toolCalls;
    }
    /**
     * Check if a response contains any tool calls.
     * @param text The text to check.
     * @returns True if the text contains tool calls, false otherwise.
     */
    static containsToolCalls(text) {
        return text.includes(this.TOOL_CALL_START) && text.includes(this.TOOL_CALL_END);
    }
    /**
     * Replace tool calls in text with a placeholder and extract them.
     * @param text The text containing tool calls.
     * @returns Object with the text with tool calls replaced and the extracted tool calls.
     */
    static extractAndReplace(text) {
        let cleanText = text;
        const toolCalls = this.parseToolCalls(text);
        // Replace each tool call with a placeholder
        for (const toolCall of toolCalls) {
            cleanText = cleanText.replace(toolCall.fullText, `[Tool Call: ${toolCall.name}]`);
        }
        return { cleanText, toolCalls };
    }
}
//# sourceMappingURL=ToolParser.js.map