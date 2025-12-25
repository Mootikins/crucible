import { Component } from 'solid-js';
import { ChatProvider } from '@/contexts/ChatContext';
import { MessageList, ChatInput } from '@/components';

const App: Component = () => {
  return (
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
  );
};

export default App;
