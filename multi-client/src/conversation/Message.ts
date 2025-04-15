import {
  BaseMessage,
  SystemMessage,
  HumanMessage,
  AIMessage,
  ToolMessage, // We'll need this later for tool results
  AIMessageChunk, // For potential streaming later
} from '@langchain/core/messages';

// Re-export core message types for use within the application
export {
  BaseMessage,
  SystemMessage,
  HumanMessage,
  AIMessage,
  ToolMessage,
  AIMessageChunk,
};

// Type alias already defined in types.ts, but good to have definitions here
export type ConversationMessage = BaseMessage;

// Helper functions to create messages (optional but can be convenient)
export function createSystemMessage(content: string): SystemMessage {
    return new SystemMessage(content);
}

export function createHumanMessage(content: string): HumanMessage {
    return new HumanMessage(content);
}

export function createAiMessage(content: string): AIMessage {
    return new AIMessage(content);
}

// We will add createToolMessage later when implementing tool calls
// export function createToolMessage(toolCallId: string, content: string): ToolMessage {
//   // Note: LangChain's ToolMessage might require specific structure or IDs.
//   // We may need to adapt this based on how tool calls are handled.
//   return new ToolMessage({ tool_call_id: toolCallId, content });
// }
