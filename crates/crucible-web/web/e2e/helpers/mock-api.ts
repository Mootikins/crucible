import type { Page } from '@playwright/test';
import { MOCK_SESSION, MOCK_PROVIDERS, MOCK_KILNS, MOCK_CONFIG, MOCK_PROJECT } from './fixtures';
import { mockSSERoute } from './mock-sse';

export interface MockOverrides {
  sessions?: object[];
  providers?: object;
  config?: object;
  kilns?: object;
  projects?: object[];
  sessionHistory?: object;
  sseEvents?: Array<{ type: string; data: object }>;
  sessionCreate?: object;
  chatMessage?: object | number;
}

export async function setupBasicMocks(page: Page, overrides: MockOverrides = {}): Promise<void> {
  await page.route('**/api/project/list', (route) =>
    route.fulfill({ json: overrides.projects ?? [MOCK_PROJECT] }),
  );

  await page.route('**/api/session/list**', (route) =>
    route.fulfill({
      json: {
        sessions: overrides.sessions ?? [MOCK_SESSION],
        total: (overrides.sessions ?? [MOCK_SESSION]).length,
      },
    }),
  );

  await page.route('**/api/session/test-session-*', async (route) => {
    if (route.request().method() === 'GET') {
      route.fulfill({ json: MOCK_SESSION });
    } else {
      route.continue();
    }
  });

  await page.route('**/api/session/*/history**', (route) =>
    route.fulfill({
      json: overrides.sessionHistory ?? {
        session_id: MOCK_SESSION.session_id,
        history: [],
        total_events: 0,
      },
    }),
  );

  await mockSSERoute(page, /\/api\/chat\/events\/.*/, overrides.sseEvents ?? []);

  await page.route('**/api/providers', (route) =>
    route.fulfill({ json: overrides.providers ?? MOCK_PROVIDERS }),
  );

  await page.route('**/api/config', (route) =>
    route.fulfill({ json: overrides.config ?? MOCK_CONFIG }),
  );

  await page.route('**/api/kilns', (route) =>
    route.fulfill({ json: overrides.kilns ?? MOCK_KILNS }),
  );

  await page.route('**/api/layout', (route) => {
    if (route.request().method() === 'GET') {
      route.fulfill({ status: 404, body: '' });
    } else {
      route.fulfill({ status: 200, body: '' });
    }
  });

  await page.route('**/api/session', async (route) => {
    if (route.request().method() === 'POST') {
      route.fulfill({ json: overrides.sessionCreate ?? MOCK_SESSION });
    } else {
      route.continue();
    }
  });

  await page.route('**/api/chat/send', async (route) => {
    if (route.request().method() === 'POST') {
      const override = overrides.chatMessage;
      if (typeof override === 'number') {
        route.fulfill({ status: override, body: 'Error' });
      } else {
        route.fulfill({ json: override ?? { message_id: 'msg-001' } });
      }
    } else {
      route.continue();
    }
  });

  await page.route('**/api/session/*/models', (route) =>
    route.fulfill({ json: { models: ['llama3.2', 'mistral'] } }),
  );
}
