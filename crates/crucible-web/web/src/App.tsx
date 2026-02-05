import { Component } from 'solid-js';
import { ProjectProvider } from '@/contexts/ProjectContext';
import { SessionProvider, useSession } from '@/contexts/SessionContext';
import { ChatProvider } from '@/contexts/ChatContext';
import { WhisperProvider } from '@/contexts/WhisperContext';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { DockLayout, ChatContent } from '@/components';

const ChatWithSession: Component = () => {
  const { currentSession } = useSession();
  
  return (
    <ChatProvider session={currentSession}>
      <DockLayout chatContent={ChatContent} />
    </ChatProvider>
  );
};

const App: Component = () => {
  return (
    <SettingsProvider>
      <ProjectProvider>
        <SessionProvider>
          <WhisperProvider>
            <ChatWithSession />
          </WhisperProvider>
        </SessionProvider>
      </ProjectProvider>
    </SettingsProvider>
  );
};

export default App;
