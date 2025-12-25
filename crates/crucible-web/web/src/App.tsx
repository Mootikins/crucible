import { Component } from 'solid-js';
import { ChatProvider } from '@/contexts/ChatContext';
import { WhisperProvider } from '@/contexts/WhisperContext';
import { MessageList, ChatInput } from '@/components';

const App: Component = () => {
  return (
    <WhisperProvider>
      <ChatProvider>
        <div class="h-full flex flex-col">
          {/* Centered container */}
          <div class="flex-1 flex flex-col w-full max-w-2xl mx-auto">
            {/* Messages area */}
            <MessageList />

            {/* Input area */}
            <ChatInput />
          </div>
        </div>
      </ChatProvider>
    </WhisperProvider>
  );
};

export default App;
