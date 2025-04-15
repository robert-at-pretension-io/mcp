import {
  BaseMessage,
  SystemMessage as LCSystemMessage,
  HumanMessage as LCHumanMessage,
  AIMessage as LCAIMessage,
  ToolMessage as LCToolMessage, // We'll need this later for tool results
  AIMessageChunk, // For potential streaming later
} from '@langchain/core/messages';

// Re-export core message types for use within the application, 
// but with our own implementation so we can add additional properties
export { BaseMessage, AIMessageChunk };

// Type alias already defined in types.ts, but good to have definitions here
export type ConversationMessage = BaseMessage;

// Extend the base LangChain message classes with our own implementations
export class SystemMessage extends LCSystemMessage {
  constructor(content: string) {
    super(content);
  }
}

export class HumanMessage extends LCHumanMessage {
  constructor(content: string) {
    super(content);
  }
}

export class AIMessage extends LCAIMessage {
  // Add optional properties for tracking tool calls
  public hasToolCalls?: boolean;
  public pendingToolCalls?: boolean;
  // Ensure additional_kwargs are passed to the super constructor

  constructor(
    content: string, 
    options?: { 
      hasToolCalls?: boolean, 
      pendingToolCalls?: boolean,
      additional_kwargs?: Record<string, any> // Add additional_kwargs here
    }
  ) {
    // Pass content and additional_kwargs to the super constructor
    // LangChain's BaseMessage constructor accepts an object or just content.
    // We need to pass the object form to include additional_kwargs.
    super({ content: content, additional_kwargs: options?.additional_kwargs || {} }); 
    this.hasToolCalls = options?.hasToolCalls || false;
    this.pendingToolCalls = options?.pendingToolCalls || false;
  }
}

export class ToolMessage extends LCToolMessage {
  constructor(
    content: string,
    toolCallId: string,
    toolName?: string
  ) {
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
export function createSystemMessage(content: string): SystemMessage {
    return new SystemMessage(content);
}

export function createHumanMessage(content: string): HumanMessage {
    return new HumanMessage(content);
}

export function createAiMessage(content: string, options?: { hasToolCalls?: boolean, pendingToolCalls?: boolean }): AIMessage {
    return new AIMessage(content, options);
}

export function createToolMessage(content: string, toolCallId: string, toolName?: string): ToolMessage {
  return new ToolMessage(content, toolCallId, toolName);
}
