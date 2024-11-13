import { Message } from './message';

export interface Room {
  id: string;
  name: string;
  isPrivate: boolean;
  hasPassword: boolean;
  messages: Message[];
  isSystem?: boolean;
} 