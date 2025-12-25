import { Component } from 'solid-js';
import { ChatProvider } from '@/contexts/ChatContext';
import { WhisperProvider } from '@/contexts/WhisperContext';
import { DockLayout, ChatContent } from '@/components';

const App: Component = () => {
  return (
    <WhisperProvider>
      <ChatProvider>
        <DockLayout chatContent={ChatContent} />
      </ChatProvider>
    </WhisperProvider>
  );
};

export default App;
