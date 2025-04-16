import {
  BaseChatModel,
} from '@langchain/core/language_models/chat_models';
// Import necessary types for RunnableInterface
import type { RunnableInterface } from "@langchain/core/runnables";
import type { BaseLanguageModelInput } from "@langchain/core/language_models/base";
import type { BaseMessageChunk } from "@langchain/core/messages";

import type { IAiClient } from './IAiClient.js';
import type { ConversationMessage } from '../conversation/Message.js';
    
export class LangchainClient implements IAiClient {
  // Accept a RunnableInterface which might be the result of model.bindTools()
  private chatModel: RunnableInterface<BaseLanguageModelInput, BaseMessageChunk>;
  private modelIdentifier: string; // The specific model identifier being used
  private providerName: string; // The provider name (e.g., "openai", "anthropic")
    
  // Update constructor parameter type
  constructor(
    chatModel: RunnableInterface<BaseLanguageModelInput, BaseMessageChunk>,
    modelIdentifier: string,
    providerName?: string
  ) {
    this.chatModel = chatModel;
    this.modelIdentifier = modelIdentifier;
    
    // Determine provider name from chat model if not explicitly provided
    if (providerName) {
      this.providerName = providerName;
    } else {
      // Try to infer from the constructor name or model name
      const constructorName = chatModel.constructor.name.toLowerCase();
      if (constructorName.includes('openai')) {
        this.providerName = 'openai';
      } else if (constructorName.includes('anthropic') || modelIdentifier.includes('claude')) {
        this.providerName = 'anthropic';
      } else if (constructorName.includes('googlegenai') || constructorName.includes('gemini')) {
        this.providerName = 'google-genai';
      } else if (constructorName.includes('mistral')) {
        this.providerName = 'mistralai';
      } else if (constructorName.includes('fireworks')) {
        this.providerName = 'fireworks';
      } else {
        this.providerName = 'unknown';
      }
    }
  }

  async generateResponse(messages: ConversationMessage[]): Promise<string> {
    try {
      // Ensure messages are in the format LangChain expects (they should be if using BaseMessage)
      const response = await this.chatModel.invoke(messages);
      
      // Handle different types of response content
      if (typeof response.content === 'string') {
        // Simple string response
        return response.content;
      } else if (Array.isArray(response.content)) {
        // Handle array content (common with Anthropic tool usage)
        // Filter for text items first, then map to get the text content
        const textItems = response.content
          .filter((item): item is { type: "text"; text: string } => // Type guard for clarity
            typeof item === 'object' && item !== null && item.type === 'text' && typeof item.text === 'string'
          );
         
        const textContent = textItems.map(item => item.text).join('\n');
           
        if (textContent) {
          // Return concatenated text parts if found
          console.log('[LangchainClient] Extracted text from complex response:', textContent);
          return textContent;
        } else if (response.content.length === 0) {
           // Handle empty array response specifically
           console.warn('[LangchainClient] AI response content was an empty array.');
           return "[AI response was empty]"; // Placeholder for empty array
        } else {
          // If array contains non-text elements (like tool_calls) but no text
          console.warn('[LangchainClient] AI response content is a non-empty array without text:', response.content);
          // Return a placeholder or stringify the structure for debugging
          return `[AI response contained non-text elements: ${JSON.stringify(response.content)}]`;
        }
      } else {
        // Handle other unexpected non-string, non-array content
        console.warn('[LangchainClient] AI response content is not a string or array:', response.content);
        return JSON.stringify(response.content); // Fallback stringify
      }
    } catch (error) {
      console.error(`Langchain AI request failed for model ${this.modelIdentifier}:`, error);
      throw new Error(`AI request failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  getModelName(): string {
    return this.modelIdentifier;
  }
  
  getProvider(): string {
    return this.providerName;
  }
}
