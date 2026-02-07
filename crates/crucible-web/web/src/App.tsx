import { Component } from 'solid-js';
import { ProjectProvider } from '@/contexts/ProjectContext';
import { SessionProvider, useSession } from '@/contexts/SessionContext';
import { ChatProvider } from '@/contexts/ChatContext';
import { WhisperProvider } from '@/contexts/WhisperContext';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { EditorProvider } from '@/contexts/EditorContext';
import { FlexLayout, ChatContent } from '@/components';

const ChatWithSession: Component = () => {
  const { currentSession, setSessionTitle } = useSession();
  
  return (
    <ChatProvider session={currentSession} setSessionTitle={setSessionTitle}>
      <FlexLayout chatContent={ChatContent} />
    </ChatProvider>
  );
};

const App: Component = () => {
  return (
    <SettingsProvider>
      <ProjectProvider>
        <EditorProvider>
          <SessionProvider>
            <WhisperProvider>
              <ChatWithSession />
            </WhisperProvider>
          </SessionProvider>
        </EditorProvider>
      </ProjectProvider>
    </SettingsProvider>
  );
};

export default App;
