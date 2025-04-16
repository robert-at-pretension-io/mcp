import { BaseMessage, SystemMessage as LCSystemMessage, HumanMessage as LCHumanMessage, AIMessage as LCAIMessage, ToolMessage as LCToolMessage, // We'll need this later for tool results
AIMessageChunk, // For potential streaming later
 } from '@langchain/core/messages';
// Re-export core message types for use within the application, 
// but with our own implementation so we can add additional properties
export { BaseMessage, AIMessageChunk };
// Extend the base LangChain message classes with our own implementations
export class SystemMessage extends LCSystemMessage {
    constructor(content) {
        super(content);
    }
}
export class HumanMessage extends LCHumanMessage {
    constructor(content) {
        super(content);
    }
}
export class AIMessage extends LCAIMessage {
    // Add optional properties for tracking tool calls
    hasToolCalls;
    pendingToolCalls;
    // Ensure additional_kwargs are passed to the super constructor
    constructor(content, // Content can be complex for tool calls
    options) {
        // LangChain's AIMessage constructor expects an AIMessageInput object.
        // Construct the input object ensuring 'content' is always present.
        const superInput = {
            content: content, // Assign the original content (string or array) directly
            additional_kwargs: options?.additional_kwargs || {},
            tool_calls: options?.tool_calls || []
        };
        super(superInput);
        // Keep our custom properties
        this.hasToolCalls = options?.hasToolCalls ?? (options?.tool_calls && options.tool_calls.length > 0);
        this.pendingToolCalls = options?.pendingToolCalls || false;
    }
}
export class ToolMessage extends LCToolMessage {
    constructor(content, toolCallId, toolName) {
        // LangChain's ToolMessage requires a specific structure
        // Mimic the actual structure needed for proper conversation history recording
        super({
            content,
            tool_call_id: toolCallId,
            name: toolName,
        });
    }
}
// Helper functions to create messages (optional but can be convenient)
export function createSystemMessage(content) {
    return new SystemMessage(content);
}
export function createHumanMessage(content) {
    return new HumanMessage(content);
}
export function createAiMessage(content, options) {
    return new AIMessage(content, options);
}
export function createToolMessage(content, toolCallId, toolName) {
    return new ToolMessage(content, toolCallId, toolName);
}
//# sourceMappingURL=Message.js.map