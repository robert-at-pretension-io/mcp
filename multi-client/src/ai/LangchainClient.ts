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

      if (typeof response.content === 'string') {
        return response.content;
      } else {
        // Handle potential non-string content (e.g., structured output)
        console.warn('AI response content is not a simple string:', response.content);
        // Attempt to stringify, or handle based on expected complex types later
        return JSON.stringify(response.content);
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
