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
    content: string | Record<string, any>[], // Content can be complex for tool calls
    options?: { 
      hasToolCalls?: boolean, 
      pendingToolCalls?: boolean,
      additional_kwargs?: Record<string, any>,
      tool_calls?: any[] // Add the standard tool_calls property
    }
  ) {
    // LangChain's AIMessage constructor expects an AIMessageInput object
    // which can include content, tool_calls, additional_kwargs etc.
    const input = typeof content === 'string' ? { content } : content; // Handle simple string or complex content
    
    super({ 
      ...input, // Spread potential complex content structure
      additional_kwargs: options?.additional_kwargs || {},
      tool_calls: options?.tool_calls || [] // Pass tool_calls to super
    }); 
    
    // Keep our custom properties
    this.hasToolCalls = options?.hasToolCalls ?? (options?.tool_calls && options.tool_calls.length > 0);
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
