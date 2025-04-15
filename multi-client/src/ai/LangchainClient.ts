import {
  BaseChatModel,
} from '@langchain/core/language_models/chat_models';
import type { IAiClient } from './IAiClient.js';
import type { ConversationMessage } from '../conversation/Message.js';

export class LangchainClient implements IAiClient {
  private chatModel: BaseChatModel;
  private modelIdentifier: string; // The specific model identifier being used

  constructor(chatModel: BaseChatModel, modelIdentifier: string) {
    this.chatModel = chatModel;
    this.modelIdentifier = modelIdentifier;
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
}
