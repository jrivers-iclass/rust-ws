import { Component, OnInit } from '@angular/core';
import { Observable } from 'rxjs';
import { ChatService } from '../../services/chat.service';
import { Room } from '../../models/room.model';
import { map } from 'rxjs/operators';

@Component({
  selector: 'app-room-list',
  templateUrl: './room-list.component.html',
  styleUrls: ['./room-list.component.scss']
})
export class RoomListComponent implements OnInit {
  rooms$: Observable<Room[]>;
  currentRoom$: Observable<Room | null>;

  constructor(private chatService: ChatService) {
    this.rooms$ = this.chatService.getRooms();
    this.currentRoom$ = this.chatService.getCurrentRoom();
  }

  ngOnInit() {
    const generalRoom: Room = {
      id: 'general',
      name: 'General',
      isSystem: true,
      isPrivate: false,
      hasPassword: false,
      messages: []
    };

    this.rooms$ = this.chatService.rooms$.pipe(
      map(rooms => {
        const existingRooms = rooms.filter(r => r.id !== 'general');
        return [generalRoom, ...existingRooms];
      })
    );
  }

  joinRoom(room: Room) {
    this.chatService.setCurrentRoom(room);
  }

  getRoomIcon(room: Room): string {
    if (room.isSystem) return 'system_update';
    if (room.isPrivate) return 'lock';
    if (room.hasPassword) return 'key';
    return 'tag';
  }
} 