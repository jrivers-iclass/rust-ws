export interface Room {
  id: string;
  name: string;
  isSystem: boolean;
  isPrivate: boolean;
  hasPassword: boolean;
  messages: Message[];
}

export interface Message {
  content: string;
  timestamp: Date;
  type: 'system' | 'user';
  sender?: string;
  username?: string;
} 