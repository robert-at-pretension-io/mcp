/**
 * Parse tool calls from LLM responses in the MCP format.
 * This is similar to the ToolParser in the Rust implementation.
 */
export interface ParsedToolCall {
    name: string;
    arguments: Record<string, any>;
    fullText: string;
}
export declare class ToolParser {
    private static readonly TOOL_CALL_START;
    private static readonly TOOL_CALL_END;
    /**
     * Parse tool calls from a text response.
     * @param text The text to parse tool calls from.
     * @returns Array of parsed tool calls.
     */
    static parseToolCalls(text: string): ParsedToolCall[];
    /**
     * Check if a response contains any tool calls.
     * @param text The text to check.
     * @returns True if the text contains tool calls, false otherwise.
     */
    static containsToolCalls(text: string): boolean;
    /**
     * Replace tool calls in text with a placeholder and extract them.
     * @param text The text containing tool calls.
     * @returns Object with the text with tool calls replaced and the extracted tool calls.
     */
    static extractAndReplace(text: string): {
        cleanText: string;
        toolCalls: ParsedToolCall[];
    };
}
