import { BaseMessage, SystemMessage as LCSystemMessage, HumanMessage as LCHumanMessage, AIMessage as LCAIMessage, ToolMessage as LCToolMessage, // We'll need this later for tool results
AIMessageChunk } from '@langchain/core/messages';
export { BaseMessage, AIMessageChunk };
export type ConversationMessage = BaseMessage;
export declare class SystemMessage extends LCSystemMessage {
    constructor(content: string);
}
export declare class HumanMessage extends LCHumanMessage {
    constructor(content: string);
}
export declare class AIMessage extends LCAIMessage {
    hasToolCalls?: boolean;
    pendingToolCalls?: boolean;
    constructor(content: string | Record<string, any>[], // Content can be complex for tool calls
    options?: {
        hasToolCalls?: boolean;
        pendingToolCalls?: boolean;
        additional_kwargs?: Record<string, any>;
        tool_calls?: any[];
    });
}
export declare class ToolMessage extends LCToolMessage {
    constructor(content: string, toolCallId: string, toolName?: string);
}
export declare function createSystemMessage(content: string): SystemMessage;
export declare function createHumanMessage(content: string): HumanMessage;
export declare function createAiMessage(content: string, options?: {
    hasToolCalls?: boolean;
    pendingToolCalls?: boolean;
}): AIMessage;
export declare function createToolMessage(content: string, toolCallId: string, toolName?: string): ToolMessage;
