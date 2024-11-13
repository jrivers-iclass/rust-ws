import { Injectable } from '@angular/core';
import { BehaviorSubject, Observable } from 'rxjs';
import { WebSocketService, WebSocketMessage, WebSocketAction } from './websocket.service';
import { Room, Message } from '../models/room.model';

@Injectable({
  providedIn: 'root'
})
export class ChatService {
  private currentRoomSubject = new BehaviorSubject<Room>({
    id: 'general',
    name: 'general',
    isSystem: false,
    isPrivate: false,
    hasPassword: false,
    messages: []
  });
  currentRoom$ = this.currentRoomSubject.asObservable();

  private roomsSubject = new BehaviorSubject<Room[]>([]);
  rooms$ = this.roomsSubject.asObservable();

  private systemMessagesSubject = new BehaviorSubject<string[]>([]);
  systemMessages$ = this.systemMessagesSubject.asObservable();

  constructor(private wsService: WebSocketService) {
    this.wsService.connect();
    this.wsService.messages$.subscribe(message => {
      console.log('WebSocket message received:', message);
      if (message.action === 'publish') {
        this.handleIncomingMessage(message);
      } else {
        console.log('Skipping non-publish message:', message.action);
      }
    });
  }

  private handleIncomingMessage(wsMessage: WebSocketMessage) {
    const currentRoom = this.currentRoomSubject.getValue();
    
    // Skip server acknowledgment messages
    if (wsMessage.message?.includes('Message sent to')) {
      return;
    }
    
    if (wsMessage.topic === currentRoom.id && wsMessage.message) {
      const newMessage: Message = {
        content: wsMessage.message,
        timestamp: new Date(wsMessage.timestamp ?? Date.now()),
        sender: wsMessage.sender || 'unknown',
        type: 'user' as const
      };

      const updatedRoom = {
        ...currentRoom,
        messages: [...(currentRoom.messages || []), newMessage]
      };
      
      this.currentRoomSubject.next(updatedRoom);
    }
  }

  getRooms(): Observable<Room[]> {
    return this.rooms$;
  }

  getCurrentRoom(): Observable<Room | null> {
    return this.currentRoom$;
  }

  setCurrentRoom(room: Room) {
    console.log('Setting current room:', room);
    
    const roomWithMessages = {
      ...room,
      messages: room.messages || []
    };
    
    this.subscribeToRoom(room.id);
    
    this.currentRoomSubject.next(roomWithMessages);
  }

  getSystemMessages(): Observable<string[]> {
    return this.systemMessages$;
  }

  publishMessage(topic: string, content: string) {
    console.log('Publishing message:', { topic, content });
    const message: WebSocketMessage = {
      action: 'publish',
      topic: topic,
      message: content
    };
    this.wsService.sendMessage(message);
  }

  async createRoom(name: string, password?: string) {
    const message: WebSocketMessage = {
      action: 'create',
      topic: name,
      ...(password && { password })
    };
    
    this.wsService.sendMessage(message);
    
    const currentRooms = this.roomsSubject.getValue();
    const newRoom: Room = {
      id: name,
      name: name,
      isSystem: false,
      isPrivate: !!password,
      hasPassword: !!password,
      messages: []
    };
    
    this.roomsSubject.next([...currentRooms, newRoom]);
  }

  subscribeToRoom(topic: string) {
    const message: WebSocketMessage = {
      action: 'subscribe',
      topic: topic
    };
    this.wsService.sendMessage(message);
  }
} 