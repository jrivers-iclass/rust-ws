import { Component, OnInit } from '@angular/core';
import { FormBuilder, FormGroup, Validators } from '@angular/forms';
import { MatDialogRef } from '@angular/material/dialog';
import { ChatService } from '../../services/chat.service';

@Component({
  selector: 'app-create-room',
  templateUrl: './create-room.component.html',
  styleUrls: ['./create-room.component.scss']
})
export class CreateRoomComponent implements OnInit {
  roomForm: FormGroup;

  constructor(
    private fb: FormBuilder,
    private chatService: ChatService,
    private dialogRef: MatDialogRef<CreateRoomComponent>
  ) {
    this.roomForm = this.fb.group({
      name: ['', [Validators.required, Validators.pattern('^[a-zA-Z0-9-_]+$')]],
      isPrivate: [false],
      password: ['']
    });
  }

  ngOnInit(): void {
    this.roomForm.get('isPrivate')?.valueChanges.subscribe(isPrivate => {
      const passwordControl = this.roomForm.get('password');
      if (isPrivate) {
        passwordControl?.setValidators([Validators.required, Validators.minLength(4)]);
      } else {
        passwordControl?.clearValidators();
      }
      passwordControl?.updateValueAndValidity();
    });
  }

  onSubmit(): void {
    if (this.roomForm.valid) {
      const { name, isPrivate, password } = this.roomForm.value;
      this.chatService.createRoom(name, isPrivate ? password : null);
      this.dialogRef.close();
    }
  }
} 